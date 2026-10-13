#!/usr/bin/env python3

import argparse
import base64
import datetime as dt
import json
import os
import re
import subprocess
import sys
import urllib.parse
import urllib.request
from pathlib import Path


API = "https://api.appstoreconnect.apple.com"
MOTE_APP_ID = "6822101041"
MOTE_BUNDLE_ID = "com.beelol.mote"
MOTE_SKU = "mote-ipad-001"
MOTE_TEAM_ID = "FQ6YGD7554"


class InventoryError(Exception):
    pass


def encoded(value):
    return base64.urlsafe_b64encode(value).rstrip(b"=").decode("ascii")


def der_integer(data, offset):
    if offset + 2 > len(data) or data[offset] != 2:
        raise InventoryError("Invalid API signature")
    length = data[offset + 1]
    start = offset + 2
    end = start + length
    if length == 0 or end > len(data):
        raise InventoryError("Invalid API signature")
    number = int.from_bytes(data[start:end], "big")
    if number.bit_length() > 256:
        raise InventoryError("Invalid API signature")
    return number.to_bytes(32, "big"), end


def es256_signature(der):
    if len(der) < 8 or der[0] != 0x30 or der[1] != len(der) - 2:
        raise InventoryError("Invalid API signature")
    r, offset = der_integer(der, 2)
    s, offset = der_integer(der, offset)
    if offset != len(der):
        raise InventoryError("Invalid API signature")
    return r + s


def bearer_token(key_path, key_id, issuer_id):
    now = int(dt.datetime.now(dt.timezone.utc).timestamp())
    header = encoded(json.dumps({"alg": "ES256", "kid": key_id, "typ": "JWT"},
                                separators=(",", ":")).encode())
    payload = encoded(json.dumps({"iss": issuer_id, "iat": now, "exp": now + 600,
                                 "aud": "appstoreconnect-v1"}, separators=(",", ":")).encode())
    body = f"{header}.{payload}"
    result = subprocess.run(["openssl", "dgst", "-sha256", "-sign", str(key_path)],
                            input=body.encode(), capture_output=True, check=False)
    if result.returncode:
        raise InventoryError("Cannot sign App Store Connect API request")
    return f"{body}.{encoded(es256_signature(result.stdout))}"


def pages(path, token):
    url = API + path
    visited = set()
    while url:
        parsed = urllib.parse.urlparse(url)
        if parsed.scheme != "https" or parsed.netloc != "api.appstoreconnect.apple.com" or url in visited:
            raise InventoryError("Unsafe or repeated App Store Connect page URL")
        visited.add(url)
        request = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}",
                                                     "Accept": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                page = json.load(response)
        except (OSError, ValueError) as error:
            raise InventoryError("Cannot fetch App Store Connect inventory") from error
        if not isinstance(page, dict) or not isinstance(page.get("data"), list):
            raise InventoryError("Invalid App Store Connect response")
        yield page
        links = page.get("links", {})
        url = links.get("next") if isinstance(links, dict) else None


def inventory(token, version, build_number):
    apps = []
    app_path = "/v1/apps?limit=200&fields%5Bapps%5D=bundleId%2Csku"
    for page in pages(app_path, token):
        for item in page["data"]:
            if not isinstance(item, dict) or not isinstance(item.get("attributes"), dict):
                raise InventoryError("Invalid app inventory row")
            attrs = item["attributes"]
            if not all(isinstance(attrs.get(key), str) and attrs[key] for key in ("bundleId", "sku")):
                raise InventoryError("Incomplete app inventory row")
            apps.append({"bundle_id": attrs["bundleId"], "sku": attrs["sku"],
                         "app_store_connect_id": str(item.get("id", ""))})
    matches = [app for app in apps if app["app_store_connect_id"] == MOTE_APP_ID]
    if len(matches) != 1 or matches[0]["bundle_id"] != MOTE_BUNDLE_ID or matches[0]["sku"] != MOTE_SKU:
        raise InventoryError("The API key cannot see the exact Mote app identity")
    if any(app is not matches[0] and (app["bundle_id"] == MOTE_BUNDLE_ID or app["sku"] == MOTE_SKU)
           for app in apps):
        raise InventoryError("Another app collides with Mote identity")
    builds = []
    build_path = (f"/v1/apps/{MOTE_APP_ID}/builds?limit=200&include=preReleaseVersion"
                  "&fields%5Bbuilds%5D=version%2CpreReleaseVersion"
                  "&fields%5BpreReleaseVersions%5D=version")
    for page in pages(build_path, token):
        included = {row.get("id"): row.get("attributes", {}).get("version")
                    for row in page.get("included", []) if isinstance(row, dict)
                    and row.get("type") == "preReleaseVersions"}
        for row in page["data"]:
            if not isinstance(row, dict) or not isinstance(row.get("attributes"), dict):
                raise InventoryError("Invalid build inventory row")
            relation = row.get("relationships", {}).get("preReleaseVersion", {}).get("data")
            release_id = relation.get("id") if isinstance(relation, dict) else None
            release_version = included.get(release_id)
            number = row["attributes"].get("version")
            if not isinstance(release_version, str) or not isinstance(number, str):
                raise InventoryError("Incomplete Mote build version")
            builds.append({"version": release_version, "build_number": number})
    matches[0]["builds"] = builds
    if any(row["version"] == version and row["build_number"] == build_number for row in builds):
        raise InventoryError("This Mote version and build number already exist")
    return {"checked_at": dt.date.today().isoformat(), "team_id": MOTE_TEAM_ID, "apps": apps}


def main():
    parser = argparse.ArgumentParser(description="Read-only Mote App Store Connect inventory")
    parser.add_argument("--version", required=True)
    parser.add_argument("--build-number", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        key_id = os.environ["MOTE_ASC_API_KEY_ID"]
        issuer = os.environ["MOTE_ASC_ISSUER_ID"]
        if re.fullmatch(r"[A-Z0-9]{10}", key_id) is None or re.fullmatch(
                r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}", issuer) is None:
            raise InventoryError("Invalid Mote API key identity")
        key_dir = Path(os.environ["API_PRIVATE_KEYS_DIR"])
        key = key_dir / f"AuthKey_{key_id}.p8"
        if not key.is_file() or key.is_symlink():
            raise InventoryError("Mote API private key is missing")
        snapshot = inventory(bearer_token(key, key_id, issuer), args.version, args.build_number)
        args.output.write_text(json.dumps(snapshot, indent=2) + "\n", encoding="utf-8")
        mote = next(app for app in snapshot["apps"] if app["app_store_connect_id"] == MOTE_APP_ID)
        print(f"Verified Mote app and {len(mote['builds'])} visible builds")
    except (KeyError, OSError, InventoryError) as error:
        print(f"Mote inventory stopped: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
