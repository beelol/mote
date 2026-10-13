#!/usr/bin/env python3

import argparse
import datetime as dt
import plistlib
import re
import sys
from pathlib import Path


TEAM = "FQ6YGD7554"
BUNDLE = "com.beelol.mote"


def inspect_profile(path, now=None):
    with Path(path).open("rb") as source:
        profile = plistlib.load(source)
    now = now or dt.datetime.now(dt.timezone.utc)
    name = profile.get("Name")
    uuid = profile.get("UUID")
    teams = profile.get("TeamIdentifier")
    expiry = profile.get("ExpirationDate")
    entitlements = profile.get("Entitlements", {})
    if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z0-9 _.-]{1,128}", name):
        raise ValueError("Invalid Mote provisioning profile name")
    if not isinstance(uuid, str) or not re.fullmatch(r"[A-Fa-f0-9-]{36}", uuid):
        raise ValueError("Invalid Mote provisioning profile UUID")
    if teams != [TEAM] or entitlements.get("application-identifier") != f"{TEAM}.{BUNDLE}":
        raise ValueError("Provisioning profile is not exclusively for the Mote app ID")
    if entitlements.get("get-task-allow") is not False:
        raise ValueError("Provisioning profile allows debugging")
    if isinstance(expiry, dt.datetime) and expiry.tzinfo is None:
        expiry = expiry.replace(tzinfo=dt.timezone.utc)
    if not isinstance(expiry, dt.datetime) or expiry <= now:
        raise ValueError("Provisioning profile is expired")
    if profile.get("ProvisionsAllDevices") or profile.get("ProvisionedDevices"):
        raise ValueError("Provisioning profile is not for App Store distribution")
    return name, uuid


def main():
    parser = argparse.ArgumentParser(description="Validate Mote-only distribution profile")
    parser.add_argument("--plist", required=True, type=Path)
    parser.add_argument("--output-directory", required=True, type=Path)
    parser.add_argument("--github-env", required=True, type=Path)
    args = parser.parse_args()
    try:
        name, uuid = inspect_profile(args.plist)
        destination = args.output_directory / f"{uuid}.mobileprovision"
        if destination.exists():
            raise ValueError("Provisioning profile destination already exists")
        args.output_directory.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(Path(args.plist).with_suffix(".mobileprovision").read_bytes())
        with args.github_env.open("a", encoding="utf-8") as env:
            env.write(f"MOTE_SIGNING_PROFILE={name}\n")
        print("Validated Mote-only App Store provisioning profile")
    except (OSError, ValueError, plistlib.InvalidFileException) as error:
        print(f"Mote provisioning stopped: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
