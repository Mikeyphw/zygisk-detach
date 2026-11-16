# Zygisk Detach: consolidated reliability and WebUI overhaul

Target: `Mikeyphw/zygisk-detach` fork of `j-hc/zygisk-detach`, upstream
v1.24.0 at `67370b17b87d1e8de0dc9393347966b87d69613f`.

## Safety design

- An unchanged legacy `detach.bin` format is validated before loading or applying.
- Every mutation takes a process-shared file lock, writes a private temporary
  file, synchronizes it, atomically renames it, and synchronizes the directory.
- Up to five permission-restricted backups are retained in
  `/data/adb/zygisk-detach/backups/`; backup rotation removes only named backups.
- Malformed on-disk state is **never reset automatically**. Restore requires
  an explicit backup index; no branch, Git, release or device state is reset.
- The WebUI reads back CLI state before reporting a saved configuration.
- A failure does not silently trigger another apply; Refresh and Restore are
  explicit choices.
- No background polling, log copying, unnecessary hook installation, or
  persistent notification service is introduced.

## CLI

```
detach detach PACKAGE...       # append selected names (atomic transaction)
detach detachall PACKAGE...    # replace selected names
detach reattach PACKAGE        # remove one package
detach list                    # saved package names
detach reset                   # clear list
detach import FILE             # import text list
detach export                  # export text list to stdout only
detach backup                  # create a private snapshot
detach backups                 # list snapshots, newest first, indexed from 0
detach restore INDEX           # restore one snapshot atomically
detach status --json           # current configuration and observational health
detach doctor --json           # diagnostics with structured output
detach doctor --bundle FILE    # private, non-overwriting JSON evidence file
detach verify                 # fail unless current Play Store hook observation exists
```

**Hook qualification**: `observed_in_current_store_process` means the module
logged a matching Binder interception for the currently running Play Store PID;
`installed_no_matching_query_observed` means hook registration succeeded but no
matching Binder query was observed; otherwise the state is unverified. The
module does not induce a synthetic Play Store transaction. This observation
is **not** proof that Google Play will never update a selected application.
Android may restrict logcat access; failures are reported as unverified.

## KernelSU WebUI

- Search/filter selected, installed and missing packages; retain missing
  package IDs to support reinstall/reconciliation.
- KernelSU's optional `getPackagesInfo` bridge supplies installed application labels;
  local `ksu://icon/<package>` resources supply actual app icons where supported.
  Older managers or missing icons fall back to monograms without network requests.
- Pending-change undo, confirmation for service/system packages, explicit
  Apply, Refresh, private Backup, Restore and export to a read-only field.
- The WebUI refreshes on foreground resume only if no pending changes exist.
- Automatic recovery does not overwrite unsaved user selections.
- Built-in diagnostic summary shows Android SDK, detected root manager/provider,
  Play Store PID and qualified hook observation.

## Compatibility and evidence

CI checks Rust codec and CLI, WebUI tests, portable native read/Binder parser
regressions and Android NDK cross-compilation for 32/64-bit Zygisk hooks plus
arm64 CLI. This is **cross-build evidence**, not a device execution test or proof of working detachment.
The installer no longer depends on the undistributed `ksu_profile` helper.
It leaves KernelSU root profiles untouched and reports when the operator must
ensure Play Store Zygisk injection in their manager. This is an explicit
compatibility choice, not an assertion that the removed helper was redundant.
The hosted workflow assembles and hash-verifies a ZIP with 32/64-bit hooks,
CLIs and bundled WebUI; its job and artifact are expressly **unqualified**
until Android device testing confirms hook behavior and installation.
Build inputs are not fully dependency-locked; byte-for-byte reproducibility
across different dependency-resolution dates remains unproven.
Before distribution: test actual KernelSU/APatch/Magisk,
Zygisk Next/ReZygisk and Play Store version combinations on physical devices.

### Local tests

```
rustc --edition=2024 --test cli/src/config.rs -o /tmp/zd-config-test && /tmp/zd-config-test
rustc --edition=2024 --test cli/src/doctor.rs -o /tmp/zd-doctor-test && /tmp/zd-doctor-test
cargo test --manifest-path cli/Cargo.toml
# Linux-only non-root CLI integration: build then run tests/cli_smoke.sh
cargo build --manifest-path cli/Cargo.toml && sh tests/cli_smoke.sh "$PWD/cli/target/debug/cli"
node --test tests/webui.test.mjs
node --check ksu-webui/src/index.js
for t in io_test codec_test parcel_test; do c++ -std=c++17 -Wall -Wextra -Werror -o "/tmp/$t" "tests/$t.cpp" && "/tmp/$t"; done
```

Device read-only evidence collection: `su -c sh tests/device_acceptance.sh` after
installing an independently qualified release. This does not install or mutate
any module. Do not interpret skipped checks as successes.

## Package build (separate from device qualification)

After native NDK/Rust builds for `arm64-v8a` and `armeabi-v7a` and WebUI
bundling, run `python3 tools/package_module.py --repo . --build-root .
--webroot .zd-webroot --output /some/new/path/zygisk-detach-UNQUALIFIED.zip`.
The assembler checks ELF class/architecture, staged metadata and WebUI, records
member SHA256s and publishes a deterministic-time ZIP, refusing existing paths.
It does NOT install, flash or mark builds release qualified. The selected Android
root manager must support a Zygisk provider and load its hook for Play Store.

KernelSU app icons use the manager-local `ksu://icon/` handler documented by
KernelSU. If it is unsupported, fallback monograms appear; no privileged icon
extraction process or network image URLs are introduced.
