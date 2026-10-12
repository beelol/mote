#!/usr/bin/env python3

import argparse
import datetime as dt
import hashlib
import json
import os
import plistlib
import re
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[2]
PROJECT = ROOT / "ios/Mote/Mote.xcodeproj"
OUTPUT_ROOT = ROOT / "target/distribution"


class ReleaseError(Exception):
    pass


def load_json(path):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ReleaseError(f"Cannot read JSON from {path}: {error}") from error


def require_string(value, label, pattern=None):
    if not isinstance(value, str) or not value or value.strip() != value:
        raise ReleaseError(f"Missing or invalid {label}")
    if pattern and re.fullmatch(pattern, value) is None:
        raise ReleaseError(f"Invalid {label}")
    return value


def validate_identity(manifest, inventory, today, need_record):
    if not isinstance(manifest, dict) or not isinstance(inventory, dict):
        raise ReleaseError("Manifest and inventory must be JSON objects")
    name = require_string(manifest.get("display_name"), "display name")
    if name != "Mote":
        raise ReleaseError("This Xcode target currently displays only Mote")
    bundle = require_string(
        manifest.get("bundle_id"), "bundle ID", r"[A-Za-z][A-Za-z0-9-]*(?:\.[A-Za-z][A-Za-z0-9-]*)+"
    )
    if bundle == "local.mote.simulator" or bundle.startswith("local.mote.simulator."):
        raise ReleaseError("Simulator bundle ID cannot be distributed")
    sku = require_string(manifest.get("sku"), "SKU", r"[A-Za-z0-9._-]{1,64}")
    team = require_string(manifest.get("team_id"), "team ID", r"[A-Z0-9]{10}")
    version = require_string(manifest.get("version"), "version", r"[0-9]+(?:\.[0-9]+){1,2}")
    build = require_string(str(manifest.get("build_number", "")), "build number", r"[1-9][0-9]*")
    record_id = manifest.get("app_store_connect_id")
    if record_id is not None:
        record_id = require_string(str(record_id), "App Store Connect ID", r"[0-9]+")
    if need_record and record_id is None:
        raise ReleaseError("A distinct App Store Connect record is required")
    checked_at = require_string(inventory.get("checked_at"), "inventory date")
    try:
        inventory_date = dt.date.fromisoformat(checked_at)
    except ValueError as error:
        raise ReleaseError("Invalid inventory date") from error
    if not dt.timedelta(0) <= today - inventory_date <= dt.timedelta(days=1):
        raise ReleaseError("App inventory must be checked within one day")
    apps = inventory.get("apps")
    if not isinstance(apps, list):
        raise ReleaseError("App inventory must contain an apps list")
    matches = []
    for app in apps:
        if not isinstance(app, dict):
            raise ReleaseError("Invalid app inventory row")
        app_bundle = require_string(app.get("bundle_id"), "inventory bundle ID")
        app_sku = require_string(app.get("sku"), "inventory SKU")
        app_id = require_string(str(app.get("app_store_connect_id", "")), "inventory app ID", r"[0-9]+")
        if app_bundle == bundle or app_sku == sku:
            if record_id is None or app_id != record_id or app_bundle != bundle or app_sku != sku:
                raise ReleaseError("Bundle ID or SKU collides with an existing app")
            matches.append(app)
    if record_id is not None and len(matches) != 1:
        raise ReleaseError("The exact Mote app record is missing or duplicated in inventory")
    if matches:
        builds = matches[0].get("builds")
        if not isinstance(builds, list):
            raise ReleaseError("The Mote app inventory must include its builds list")
        if any(str(row.get("version")) == version and str(row.get("build_number")) == build
               for row in builds if isinstance(row, dict)):
            raise ReleaseError("Build number already exists for this app version")
    return {"display_name": name, "bundle_id": bundle, "sku": sku, "team_id": team,
            "version": version, "build_number": build, "app_store_connect_id": record_id}


def validate_output(raw, must_be_new):
    path = Path(raw)
    if not path.is_absolute():
        raise ReleaseError("Output path must be absolute")
    if ".." in path.parts:
        raise ReleaseError("Output path may not traverse a parent directory")
    if path == OUTPUT_ROOT or OUTPUT_ROOT not in path.parents:
        raise ReleaseError(f"Output path must be below {OUTPUT_ROOT}")
    if any(part.is_symlink() for part in [path, *path.parents]):
        raise ReleaseError("Output path may not contain a symlink")
    if path.exists() and must_be_new:
        raise ReleaseError("Output path already exists")
    if not path.exists() and not must_be_new:
        raise ReleaseError("Output path does not exist")
    return path


def tool_output(command):
    try:
        result = subprocess.run(command, capture_output=True, check=True)
    except (OSError, subprocess.CalledProcessError) as error:
        raise ReleaseError(f"Required tool failed: {command[0]}") from error
    return result.stdout.decode("utf-8", "replace")


def signing_preflight(team):
    identities = tool_output(["security", "find-identity", "-v", "-p", "codesigning"])
    if re.search(r"Apple Distribution: [^\n]*\(" + re.escape(team) + r"\)", identities) is None:
        raise ReleaseError("No local Apple Distribution signing identity for the selected team")


def preflight(identity):
    xcode = tool_output(["xcodebuild", "-version"]).strip().replace("\n", ", ")
    sdk = tool_output(["xcrun", "--sdk", "iphoneos", "--show-sdk-version"]).strip()
    targets = tool_output(["rustup", "target", "list", "--installed"])
    if "aarch64-apple-ios\n" not in targets + "\n":
        raise ReleaseError("The aarch64-apple-ios Rust target is not installed")
    signing_preflight(identity["team_id"])
    print(f"Preflight passed: {xcode}, iPhoneOS SDK {sdk}, aarch64-apple-ios")
    print(f"Mote: {identity['bundle_id']}, team {identity['team_id']}, "
          f"version {identity['version']} ({identity['build_number']})")
    print("Inventory is a supplied snapshot. Confirm it is complete in App Store Connect.")


def run_mutation(command, label):
    try:
        result = subprocess.run(command, capture_output=True, check=False)
    except OSError as error:
        raise ReleaseError(f"{label} could not start") from error
    if result.returncode:
        raise ReleaseError(f"{label} failed with exit status {result.returncode}. Check its state before retrying.")


def read_bundle_info(path, identity):
    with Path(path).open("rb") as source:
        info = plistlib.load(source)
    expected = {"CFBundleIdentifier": identity["bundle_id"],
                "CFBundleShortVersionString": identity["version"],
                "CFBundleVersion": identity["build_number"],
                "CFBundleDisplayName": identity["display_name"]}
    for key, value in expected.items():
        if str(info.get(key)) != value:
            raise ReleaseError(f"Exported {key} does not match the approved identity")


def verify_signing(app, identity):
    tool_output(["codesign", "--verify", "--deep", "--strict", str(app)])
    details = subprocess.run(["codesign", "-dv", "--verbose=4", str(app)],
                             capture_output=True, check=False)
    if details.returncode or f"TeamIdentifier={identity['team_id']}" not in (
            details.stdout + details.stderr).decode("utf-8", "replace"):
        raise ReleaseError("Code signature has the wrong team")
    result = subprocess.run(["codesign", "-d", "--entitlements", "-", str(app)],
                            capture_output=True, check=False)
    data = result.stdout + result.stderr
    begin = data.find(b"<plist")
    end = data.find(b"</plist>", begin)
    if result.returncode or begin < 0 or end < 0:
        raise ReleaseError("Signed app entitlements could not be inspected")
    entitlements = plistlib.loads(data[begin:end + len(b"</plist>")])
    if entitlements.get("application-identifier") != (
            identity["team_id"] + "." + identity["bundle_id"]):
        raise ReleaseError("Signed app entitlement has the wrong application identifier")
    if entitlements.get("get-task-allow") is not False:
        raise ReleaseError("Distribution app must not allow debugging")


def verify_ipa(ipa, identity):
    with zipfile.ZipFile(ipa) as package:
        names = package.namelist()
        if any(PurePosixPath(name).is_absolute() or ".." in PurePosixPath(name).parts for name in names):
            raise ReleaseError("IPA contains an unsafe path")
        plist_name = "Payload/Mote.app/Info.plist"
        if plist_name not in names:
            raise ReleaseError("IPA lacks the Mote app")
        with tempfile.TemporaryDirectory(prefix="mote-ipa-") as temporary:
            package.extractall(temporary)
            app = Path(temporary) / "Payload/Mote.app"
            read_bundle_info(app / "Info.plist", identity)
            verify_signing(app, identity)


def archive_export(identity, output):
    output.mkdir(parents=True, exist_ok=False)
    archive = output / "Mote.xcarchive"
    export = output / "export"
    options = output / "ExportOptions.plist"
    with options.open("wb") as target:
        plistlib.dump({"method": "app-store-connect", "destination": "export",
                      "signingStyle": "automatic", "teamID": identity["team_id"],
                      "distributionBundleIdentifier": identity["bundle_id"],
                      "manageAppVersionAndBuildNumber": False,
                      "testFlightInternalTestingOnly": True}, target)
    run_mutation(["xcodebuild", "-project", str(PROJECT), "-scheme", "Mote", "-configuration",
                  "Release", "-destination", "generic/platform=iOS", "-derivedDataPath",
                  str(output / "DerivedData"), "-archivePath", str(archive),
                  "CODE_SIGNING_ALLOWED=YES", "CODE_SIGN_STYLE=Automatic",
                  "CODE_SIGN_IDENTITY=Apple Distribution",
                  "DEVELOPMENT_TEAM=" + identity["team_id"],
                  "PRODUCT_BUNDLE_IDENTIFIER=" + identity["bundle_id"],
                  "MARKETING_VERSION=" + identity["version"],
                  "CURRENT_PROJECT_VERSION=" + identity["build_number"], "archive"], "Archive")
    app = archive / "Products/Applications/Mote.app"
    read_bundle_info(app / "Info.plist", identity)
    verify_signing(app, identity)
    run_mutation(["xcodebuild", "-exportArchive", "-archivePath", str(archive),
                  "-exportPath", str(export), "-exportOptionsPlist", str(options)], "IPA export")
    ipa = export / "Mote.ipa"
    if not ipa.is_file():
        raise ReleaseError("Export did not produce Mote.ipa")
    verify_ipa(ipa, identity)
    receipt = {"identity": identity, "ipa_sha256": hashlib.sha256(ipa.read_bytes()).hexdigest()}
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(f"Signed archive and IPA verified at {output}. No upload was attempted.")


def upload(identity, output):
    receipt = load_json(output / "receipt.json")
    if receipt.get("identity") != identity:
        raise ReleaseError("Export receipt does not match the approved identity")
    ipa = output / "export/Mote.ipa"
    if not ipa.is_file() or hashlib.sha256(ipa.read_bytes()).hexdigest() != receipt.get("ipa_sha256"):
        raise ReleaseError("Exported IPA is missing or has changed")
    verify_ipa(ipa, identity)
    key = require_string(os.environ.get("MOTE_ASC_API_KEY_ID"), "App Store Connect API key ID")
    issuer = require_string(os.environ.get("MOTE_ASC_ISSUER_ID"), "App Store Connect issuer ID")
    private_keys = Path(require_string(os.environ.get("API_PRIVATE_KEYS_DIR"), "private-key directory"))
    if not private_keys.is_absolute() or ".." in private_keys.parts:
        raise ReleaseError("Private-key directory must be an absolute path without traversal")
    try:
        private_keys = private_keys.resolve(strict=True)
    except (OSError, RuntimeError) as error:
        raise ReleaseError("Private-key directory cannot be resolved") from error
    if private_keys == ROOT or ROOT in private_keys.parents or not private_keys.is_dir():
        raise ReleaseError("Private keys must be in an existing directory outside the repository")
    run_mutation(["xcrun", "altool", "--upload-app", "-f", str(ipa), "--api-key", key,
                  "--api-issuer", issuer], "Upload")
    print("Upload command succeeded. Confirm processing in App Store Connect before TestFlight testing.")


def main():
    parser = argparse.ArgumentParser(description="Guarded Mote internal TestFlight distribution")
    parser.add_argument("action", choices=["preflight", "archive-export", "upload"])
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--inventory", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--confirm-inventory-current", action="store_true")
    parser.add_argument("--confirm-physical-device-verified", action="store_true")
    parser.add_argument("--confirm-upload", action="store_true")
    args = parser.parse_args()
    try:
        identity = validate_identity(load_json(args.manifest), load_json(args.inventory),
                                     dt.date.today(), args.action != "preflight")
        output = validate_output(args.output, args.action != "upload")
        if args.action != "preflight" and not args.confirm_inventory_current:
            raise ReleaseError("Confirm the supplied inventory is complete and current")
        if args.action != "preflight" and not args.confirm_physical_device_verified:
            raise ReleaseError("Confirm the physical iPad UI checks passed before distribution")
        if args.action == "upload" and not args.confirm_upload:
            raise ReleaseError("Upload requires --confirm-upload")
        preflight(identity)
        if args.action == "archive-export":
            archive_export(identity, output)
        elif args.action == "upload":
            upload(identity, output)
    except ReleaseError as error:
        print(f"Mote distribution stopped: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
