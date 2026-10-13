# IPAD-12 verification in progress

Date: 2026-10-12, America/Los_Angeles. Environment: macOS with Xcode 27.0, Python 3, local Mote repository. The GitHub Actions workflow has not been dispatched.

Static inspection: `.github/workflows/mote-testflight.yml` contains only `workflow_dispatch`. The build job requires `refs/heads/main` and has no Apple credentials. Upload defaults to false. The separate upload job requires the `mote-testflight` environment and an explicit native UI verification input. The Mote identity is fixed at Apple ID `6822101041`, bundle `com.beelol.mote`, SKU `mote-ipad-001`, team `FQ6YGD7554`. The workflow uses only Mote-prefixed release secrets and a Mote-only provisioning profile. It does not reference another app's bundle, profile, SKU or TestFlight group.

Command: `python3 -B -m unittest discover -s ios/Mote -p 'test_*.py'`.

Output: `Ran 20 tests ... OK`. The tests cover inventory identity mismatch, duplicate build refusal, Mote-only profile rejection of another bundle, manual export profile binding and the existing distribution guards. No Apple mutation occurred during these tests.

Command: `cargo test --workspace`. Output: passed with exit status 0 when run with local socket access. The first restricted-sandbox attempt failed only because the existing browser fixture could not bind a localhost socket. The rerun completed all workspace and doc tests. Live tests marked `MOTE_LIVE=1` remained ignored.

Command: `cargo clippy --workspace -- -D warnings`. Output: passed with exit status 0 and no warnings.

Command: `ruby -e 'require "yaml"; p YAML.load_file(".github/workflows/mote-testflight.yml").keys'`. Output: `["name", "on", "permissions", "concurrency", "jobs"]`. This is a syntax check, not a hosted GitHub Actions run.

Pending: run one build-only dispatch from `main` and inspect its unsigned iPad build result. Configure the protected GitHub environment and dedicated Mote release credentials. Run one deliberate upload dispatch using a fresh Mote build number. Confirm processing and the internal group in App Store Connect, and confirm other apps and groups are unchanged. Until then this criterion remains open. The local checks do not prove the hosted runner's signing and upload path works.
