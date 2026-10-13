import datetime as dt
import importlib.util
import plistlib
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(f"{name}.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


inventory = load("ci_inventory")
profile = load("ci_profile")
release = load("distribute")


class ManualReleaseTests(unittest.TestCase):
    def test_apple_signature_is_raw_es256(self):
        der = bytes([0x30, 0x44, 0x02, 0x20]) + bytes(range(1, 33)) + bytes([0x02, 0x20]) + bytes(range(33, 65))
        self.assertEqual(inventory.es256_signature(der), bytes(range(1, 65)))
        with self.assertRaises(inventory.InventoryError):
            inventory.es256_signature(der + b"x")

    def test_inventory_requires_exact_mote_record_and_unique_build(self):
        apps = {"data": [{"id": inventory.MOTE_APP_ID, "attributes": {
            "bundleId": inventory.MOTE_BUNDLE_ID, "sku": inventory.MOTE_SKU}}]}
        builds = {"data": [{"attributes": {"version": "1"}, "relationships": {
            "preReleaseVersion": {"data": {"id": "release"}}}}],
            "included": [{"type": "preReleaseVersions", "id": "release", "attributes": {"version": "0.1.0"}}]}
        with patch.object(inventory, "pages", side_effect=[[apps], [builds]]):
            snapshot = inventory.inventory("token", "0.1.0", "2")
        self.assertEqual(snapshot["apps"][0]["builds"], [{"version": "0.1.0", "build_number": "1"}])
        self.assertEqual(release.validate_identity({"display_name": "Mote", "bundle_id": "com.beelol.mote",
                                                    "sku": "mote-ipad-001", "team_id": "FQ6YGD7554",
                                                    "app_store_connect_id": "6822101041", "version": "0.1.0",
                                                    "build_number": "2"}, snapshot, dt.date.today(), True)["bundle_id"],
                         inventory.MOTE_BUNDLE_ID)
        with patch.object(inventory, "pages", side_effect=[[apps], [builds]]):
            with self.assertRaisesRegex(inventory.InventoryError, "already exist"):
                inventory.inventory("token", "0.1.0", "1")
        apps["data"][0]["attributes"]["bundleId"] = "com.other.app"
        with patch.object(inventory, "pages", side_effect=[[apps]]):
            with self.assertRaisesRegex(inventory.InventoryError, "exact Mote"):
                inventory.inventory("token", "0.1.0", "2")

    def test_profile_is_exclusive_and_distribution_only(self):
        now = dt.datetime(2026, 10, 12, tzinfo=dt.timezone.utc)
        data = {"Name": "Mote App Store", "UUID": "12345678-1234-1234-1234-123456789abc",
                "TeamIdentifier": ["FQ6YGD7554"], "ExpirationDate": now + dt.timedelta(days=1),
                "Entitlements": {"application-identifier": "FQ6YGD7554.com.beelol.mote",
                                 "get-task-allow": False}}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "profile.plist"
            path.write_bytes(plistlib.dumps(data))
            self.assertEqual(profile.inspect_profile(path, now)[0], "Mote App Store")
            data["Entitlements"]["application-identifier"] = "FQ6YGD7554.com.beelol.overseer"
            path.write_bytes(plistlib.dumps(data))
            with self.assertRaisesRegex(ValueError, "exclusively"):
                profile.inspect_profile(path, now)

    def test_manual_signing_targets_only_mote_profile(self):
        identity = {"bundle_id": "com.beelol.mote", "team_id": "FQ6YGD7554"}
        options = release.export_options(identity, "upload", "Mote App Store")
        self.assertEqual(options["signingStyle"], "manual")
        self.assertEqual(options["provisioningProfiles"], {"com.beelol.mote": "Mote App Store"})
        self.assertTrue(options["testFlightInternalTestingOnly"])


if __name__ == "__main__":
    unittest.main()
