import datetime as dt
import importlib.util
import json
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


if __name__ == "__main__":
    unittest.main()
