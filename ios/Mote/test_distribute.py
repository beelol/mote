import datetime as dt
import hashlib
import importlib.util
import json
import os
import plistlib
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("mote_distribute", Path(__file__).with_name("distribute.py"))
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class DistributionPreflightTests(unittest.TestCase):
    def setUp(self):
        self.today = dt.date(2026, 10, 11)
        self.manifest = {
            "display_name": "Mote",
            "bundle_id": "com.example.mote",
            "sku": "MOTE-NEW",
            "team_id": "ABCDE12345",
            "version": "0.1.0",
            "build_number": 7,
            "app_store_connect_id": "1234567890",
        }
        self.inventory = {
            "checked_at": "2026-10-11",
            "team_id": "ABCDE12345",
            "apps": [{"bundle_id": "com.example.mote", "sku": "MOTE-NEW",
                      "app_store_connect_id": "1234567890", "builds": []}],
        }

    def valid(self):
        return release.validate_identity(self.manifest, self.inventory, self.today, True)

    def test_registered_identity_passes_without_apple_mutation(self):
        self.assertEqual(self.valid()["bundle_id"], "com.example.mote")

    def test_missing_and_simulator_identity_fail_closed(self):
        self.manifest.pop("team_id")
        with self.assertRaises(release.ReleaseError):
            self.valid()
        self.manifest["team_id"] = "ABCDE12345"
        self.manifest["bundle_id"] = "local.mote.simulator"
        with self.assertRaises(release.ReleaseError):
            self.valid()

    def test_colliding_bundle_or_sku_fails_closed(self):
        self.inventory["apps"][0]["app_store_connect_id"] = "9999999999"
        with self.assertRaises(release.ReleaseError):
            self.valid()
        self.inventory["apps"][0]["bundle_id"] = "com.other.app"
        with self.assertRaises(release.ReleaseError):
            self.valid()

    def test_missing_record_and_stale_inventory_fail_closed(self):
        self.manifest.pop("app_store_connect_id")
        with self.assertRaises(release.ReleaseError):
            self.valid()
        self.manifest["app_store_connect_id"] = "1234567890"
        self.inventory["checked_at"] = "2026-10-09"
        with self.assertRaises(release.ReleaseError):
            self.valid()

    def test_duplicate_build_number_fails_closed(self):
        self.inventory["apps"][0]["builds"] = [{"version": "0.1.0", "build_number": 7}]
        with self.assertRaises(release.ReleaseError):
            self.valid()

    def test_inventory_must_belong_to_the_selected_signing_team(self):
        self.inventory["team_id"] = "DIFFERENT1"
        with self.assertRaises(release.ReleaseError):
            self.valid()
        self.inventory.pop("team_id")
        with self.assertRaises(release.ReleaseError):
            self.valid()

    def test_absent_signing_identity_fails_closed(self):
        with patch.object(release, "tool_output", return_value="0 valid identities found"):
            with self.assertRaises(release.ReleaseError):
                release.signing_preflight("ABCDE12345")

    def test_bad_output_paths_fail_closed(self):
        with self.assertRaises(release.ReleaseError):
            release.validate_output(Path("relative"), True)
        with self.assertRaises(release.ReleaseError):
            release.validate_output(Path(tempfile.gettempdir()) / "other-app", True)
        with self.assertRaises(release.ReleaseError):
            release.validate_output(release.OUTPUT_ROOT, True)
        with self.assertRaises(release.ReleaseError):
            release.validate_output(release.OUTPUT_ROOT / "nested/../../outside", True)
        with tempfile.TemporaryDirectory(dir=release.OUTPUT_ROOT.parent) as directory:
            outside = Path(directory) / "existing"
            outside.mkdir()
            with self.assertRaises(release.ReleaseError):
                release.validate_output(outside, True)

    def test_archive_requires_explicit_physical_and_inventory_confirmations(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "manifest.json"
            inventory = Path(directory) / "inventory.json"
            manifest.write_text(json.dumps(self.manifest), encoding="utf-8")
            inventory.write_text(json.dumps(self.inventory), encoding="utf-8")
            arguments = ["distribute.py", "archive-export", "--manifest", str(manifest),
                         "--inventory", str(inventory), "--output",
                         str(release.OUTPUT_ROOT / "test-archive")]
            with patch("sys.argv", arguments), patch.object(release, "preflight") as preflight, \
                    patch.object(release, "archive_export") as archive:
                self.assertEqual(release.main(), 1)
                preflight.assert_not_called()
                archive.assert_not_called()
            with patch("sys.argv", arguments + ["--confirm-inventory-current"]), \
                    patch.object(release, "preflight") as preflight, \
                    patch.object(release, "archive_export") as archive:
                self.assertEqual(release.main(), 1)
                preflight.assert_not_called()
                archive.assert_not_called()

    def test_xcode_entitlements_are_parsed_as_plist(self):
        identity = self.valid()
        entitlements = plistlib.dumps({
            "application-identifier": "ABCDE12345.com.example.mote",
            "get-task-allow": False,
        })
        results = [
            subprocess.CompletedProcess([], 0, b"", b"TeamIdentifier=ABCDE12345\n"),
            subprocess.CompletedProcess([], 0, entitlements, b""),
        ]
        with patch.object(release, "tool_output"), patch.object(release.subprocess, "run", side_effect=results) as run:
            release.verify_signing(Path("/private/tmp/Mote.app"), identity)
        self.assertEqual(run.call_args_list[1].args[0][3], ":-")

    def test_export_without_ipad_icon_fails_closed(self):
        identity = self.valid()
        with tempfile.TemporaryDirectory() as directory:
            app = Path(directory) / "Mote.app"
            app.mkdir()
            info = app / "Info.plist"
            with info.open("wb") as target:
                plistlib.dump({"CFBundleIdentifier": identity["bundle_id"],
                              "CFBundleShortVersionString": identity["version"],
                              "CFBundleVersion": identity["build_number"],
                              "CFBundleDisplayName": identity["display_name"]}, target)
            with self.assertRaisesRegex(release.ReleaseError, "iPad app icon"):
                release.read_bundle_info(info, identity)

    def test_archive_digest_refuses_external_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app = root / "Mote.app"
            app.mkdir()
            outside = root / "outside"
            outside.write_bytes(b"outside")
            (app / "outside-link").symlink_to(outside)
            with self.assertRaisesRegex(release.ReleaseError, "external symlink"):
                release.archive_app_digest(app)

    def test_archive_export_uses_internal_only_export_and_no_upload(self):
        identity = self.valid()
        commands = []
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "new-build"

            def fake_mutation(command, label):
                commands.append((command, label))
                if label == "Archive":
                    app = output / "Mote.xcarchive/Products/Applications/Mote.app"
                    app.mkdir(parents=True)
                    with (app / "Info.plist").open("wb") as target:
                        plistlib.dump({"CFBundleIdentifier": identity["bundle_id"],
                                      "CFBundleShortVersionString": identity["version"],
                                      "CFBundleVersion": identity["build_number"],
                                      "CFBundleDisplayName": identity["display_name"],
                                      "CFBundleIcons~ipad": {"CFBundlePrimaryIcon": {
                                          "CFBundleIconName": "AppIcon"}}}, target)
                    (app / "Assets.car").write_bytes(b"fixture icon")
                else:
                    export = output / "export"
                    export.mkdir()
                    (export / "Mote.ipa").write_bytes(b"fixture ipa")

            with patch.object(release, "run_mutation", side_effect=fake_mutation), \
                    patch.object(release, "verify_signing"), patch.object(release, "verify_ipa"):
                release.archive_export(identity, output)
            self.assertEqual([label for _, label in commands], ["Archive", "IPA export"])
            archive_command, export_command = [command for command, _ in commands]
            self.assertIn("CODE_SIGNING_ALLOWED=YES", archive_command)
            self.assertIn("DEVELOPMENT_TEAM=ABCDE12345", archive_command)
            self.assertIn("PRODUCT_BUNDLE_IDENTIFIER=com.example.mote", archive_command)
            self.assertIn("MARKETING_VERSION=0.1.0", archive_command)
            self.assertIn("CURRENT_PROJECT_VERSION=7", archive_command)
            self.assertIn("archive", archive_command)
            self.assertIn("-exportArchive", export_command)
            self.assertFalse(any("altool" in command for command in (archive_command, export_command)))
            with (output / "ExportOptions.plist").open("rb") as source:
                options = plistlib.load(source)
            self.assertEqual(options["method"], "app-store-connect")
            self.assertEqual(options["destination"], "export")
            self.assertTrue(options["testFlightInternalTestingOnly"])
            self.assertFalse(options["manageAppVersionAndBuildNumber"])
            self.assertEqual(options["distributionBundleIdentifier"], identity["bundle_id"])
            receipt = release.load_json(output / "receipt.json")
            self.assertEqual(receipt["identity"], identity)
            self.assertEqual(receipt["ipa_sha256"], hashlib.sha256(b"fixture ipa").hexdigest())
            self.assertEqual(receipt["archive_app_sha256"], release.archive_app_digest(
                output / "Mote.xcarchive/Products/Applications/Mote.app"))

    def test_upload_is_separate_and_refuses_a_changed_ipa(self):
        identity = self.valid()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            export = output / "export"
            export.mkdir()
            ipa = export / "Mote.ipa"
            ipa.write_bytes(b"changed")
            (output / "receipt.json").write_text(json.dumps({
                "identity": identity, "ipa_sha256": hashlib.sha256(b"original").hexdigest()
            }), encoding="utf-8")
            with patch.object(release, "run_mutation") as run:
                with self.assertRaises(release.ReleaseError):
                    release.upload(identity, output)
                run.assert_not_called()

    def test_upload_rechecks_archive_and_uses_internal_only_xcode_upload(self):
        identity = self.valid()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "release"
            export = output / "export"
            export.mkdir(parents=True)
            ipa = export / "Mote.ipa"
            ipa.write_bytes(b"signed fixture")
            app = output / "Mote.xcarchive/Products/Applications/Mote.app"
            app.mkdir(parents=True)
            with (app / "Info.plist").open("wb") as target:
                plistlib.dump({"CFBundleIdentifier": identity["bundle_id"],
                              "CFBundleShortVersionString": identity["version"],
                              "CFBundleVersion": identity["build_number"],
                              "CFBundleDisplayName": identity["display_name"],
                              "CFBundleIcons~ipad": {"CFBundlePrimaryIcon": {
                                  "CFBundleIconName": "AppIcon"}}}, target)
            (app / "Assets.car").write_bytes(b"fixture icon")
            (output / "receipt.json").write_text(json.dumps({
                "identity": identity,
                "ipa_sha256": hashlib.sha256(b"signed fixture").hexdigest(),
                "archive_app_sha256": release.archive_app_digest(app),
            }), encoding="utf-8")
            keys = Path(directory) / "keys"
            keys.mkdir()
            (keys / "AuthKey_TESTKEY123.p8").write_text("fixture key", encoding="utf-8")
            env = {"MOTE_ASC_API_KEY_ID": "TESTKEY123",
                   "MOTE_ASC_ISSUER_ID": "12345678-1234-1234-1234-123456789abc",
                   "API_PRIVATE_KEYS_DIR": str(keys)}
            with patch.dict(os.environ, env), patch.object(release, "verify_ipa"), \
                    patch.object(release, "verify_signing") as signing, \
                    patch.object(release, "run_mutation") as run:
                release.upload(identity, output)
                with self.assertRaisesRegex(release.ReleaseError, "already exists"):
                    release.upload(identity, output)
            signing.assert_called_once_with(app, identity)
            command, label = run.call_args.args
            self.assertEqual(label, "Upload")
            self.assertEqual(command[:2], ["xcodebuild", "-exportArchive"])
            self.assertEqual(command[command.index("-archivePath") + 1], str(output / "Mote.xcarchive"))
            self.assertEqual(command[command.index("-authenticationKeyPath") + 1],
                             str((keys / "AuthKey_TESTKEY123.p8").resolve()))
            self.assertNotIn("fixture key", command)
            with (output / "UploadOptions.plist").open("rb") as source:
                options = plistlib.load(source)
            self.assertEqual(options["destination"], "upload")
            self.assertEqual(options["teamID"], identity["team_id"])
            self.assertEqual(options["distributionBundleIdentifier"], identity["bundle_id"])
            self.assertTrue(options["testFlightInternalTestingOnly"])
            self.assertFalse(options["manageAppVersionAndBuildNumber"])
            self.assertEqual(release.load_json(output / "upload-attempt.json")["identity"], identity)

    def test_upload_refuses_missing_archive_or_key_without_mutation(self):
        identity = self.valid()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "release"
            export = output / "export"
            export.mkdir(parents=True)
            ipa = export / "Mote.ipa"
            ipa.write_bytes(b"signed fixture")
            (output / "receipt.json").write_text(json.dumps({
                "identity": identity,
                "ipa_sha256": hashlib.sha256(b"signed fixture").hexdigest(),
            }), encoding="utf-8")
            with patch.object(release, "verify_ipa"), patch.object(release, "run_mutation") as run:
                with self.assertRaisesRegex(release.ReleaseError, "archive is missing"):
                    release.upload(identity, output)
                run.assert_not_called()
            app = output / "Mote.xcarchive/Products/Applications/Mote.app"
            app.mkdir(parents=True)
            with (app / "Info.plist").open("wb") as target:
                plistlib.dump({"CFBundleIdentifier": identity["bundle_id"],
                              "CFBundleShortVersionString": identity["version"],
                              "CFBundleVersion": identity["build_number"],
                              "CFBundleDisplayName": identity["display_name"],
                              "CFBundleIcons~ipad": {"CFBundlePrimaryIcon": {
                                  "CFBundleIconName": "AppIcon"}}}, target)
            (app / "Assets.car").write_bytes(b"fixture icon")
            (output / "receipt.json").write_text(json.dumps({
                "identity": identity,
                "ipa_sha256": hashlib.sha256(b"signed fixture").hexdigest(),
                "archive_app_sha256": release.archive_app_digest(app),
            }), encoding="utf-8")
            keys = Path(directory) / "keys"
            keys.mkdir()
            env = {"MOTE_ASC_API_KEY_ID": "TESTKEY123",
                   "MOTE_ASC_ISSUER_ID": "12345678-1234-1234-1234-123456789abc",
                   "API_PRIVATE_KEYS_DIR": str(keys)}
            with patch.dict(os.environ, env), patch.object(release, "verify_ipa"), \
                    patch.object(release, "verify_signing"), \
                    patch.object(release, "run_mutation") as run:
                with self.assertRaisesRegex(release.ReleaseError, "private key file"):
                    release.upload(identity, output)
                run.assert_not_called()
            (keys / "AuthKey_TESTKEY123.p8").write_text("fixture key", encoding="utf-8")
            (app / "Assets.car").write_bytes(b"changed icon")
            with patch.dict(os.environ, env), patch.object(release, "verify_ipa"), \
                    patch.object(release, "verify_signing"), \
                    patch.object(release, "run_mutation") as run:
                with self.assertRaisesRegex(release.ReleaseError, "archive has changed"):
                    release.upload(identity, output)
                run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
