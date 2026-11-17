"""Executable negative and positive tests for hosted Android SDK provisioning.

Never installs SDKs on the testing host. All SDK roots and sdkmanager commands are
isolated disposable fixtures. Source controls both mock and test assertions.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[1]
HELPER = ROOT / 'tools' / 'ci_android_sdk.sh'
VERSION = '27.2.12479018'

class AndroidSDKBootstrapTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='zd-android-sdk-ci-')
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        self.sdk = self.home / 'android sdk'
        self.sdk.mkdir()
        self.env_file = self.home / 'github-env'
        self.env_file.write_text('')
        self.args = self.home / 'sdkmanager-arguments'
        self.env = os.environ.copy()
        self.env['ANDROID_SDK_ROOT'] = str(self.sdk)
        self.env['ANDROID_HOME'] = str(self.sdk)
        self.env['GITHUB_ENV'] = str(self.env_file)
        self.env['ZD_FIXTURE_ARGS'] = str(self.args)

    def manager(self, creates=True, wrong_version=False):
        manager = self.sdk / 'cmdline-tools/latest/bin/sdkmanager'
        manager.parent.mkdir(parents=True, exist_ok=True)
        manager.write_text(textwrap.dedent('''\
            #!/usr/bin/env bash
            set -euo pipefail
            printf '%s\n' "$*" >> "$ZD_FIXTURE_ARGS"
            if [[ "$1" == --version ]]; then echo '16.0'; exit 0; fi
            if [[ "$1" == "--sdk_root=$ANDROID_SDK_ROOT" && "$2" == --install && "$3" == 'ndk;27.2.12479018' ]]; then
                [[ "${ZD_FIXTURE_INSTALL:-yes}" == yes ]] || exit 11
                mkdir -p "$ANDROID_SDK_ROOT/ndk/27.2.12479018"
                printf '#!/bin/sh\nexit 0\n' > "$ANDROID_SDK_ROOT/ndk/27.2.12479018/ndk-build"
                chmod 755 "$ANDROID_SDK_ROOT/ndk/27.2.12479018/ndk-build"
                printf 'Pkg.Revision = %s\n' "${ZD_FIXTURE_REVISION:-27.2.12479018}" > "$ANDROID_SDK_ROOT/ndk/27.2.12479018/source.properties"
                exit 0
            fi
            exit 12
        '''))
        manager.chmod(0o755)

    def run_helper(self):
        return subprocess.run(['bash', str(HELPER)], env=self.env, capture_output=True, text=True, timeout=20)

    def test_fresh_runner_installs_only_pinned_ndk(self):
        self.manager()
        p = self.run_helper()
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn('ZD_ANDROID_SDK_PASS', p.stdout)
        calls = self.args.read_text().splitlines()
        self.assertEqual(calls, ['--version', f'--sdk_root={self.sdk} --install ndk;{VERSION}'])
        self.assertIn(f'ANDROID_NDK_HOME={self.sdk}/ndk/{VERSION}', self.env_file.read_text())
        self.assertNotIn('sdkmanager tools', HELPER.read_text())

    def test_preinstalled_pinned_ndk_skips_network_install(self):
        self.manager()
        ndk = self.sdk / 'ndk' / VERSION
        ndk.mkdir(parents=True)
        (ndk/'ndk-build').write_text('#!/bin/sh\nexit 0\n')
        (ndk/'ndk-build').chmod(0o755)
        (ndk/'source.properties').write_text(f'Pkg.Revision = {VERSION}\n')
        p = self.run_helper()
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(self.args.read_text().splitlines(), ['--version'])

    def test_sdk_root_missing_fails_closed(self):
        self.env['ANDROID_SDK_ROOT'] = str(self.home/'missing')
        p = self.run_helper()
        self.assertNotEqual(p.returncode, 0)
        self.assertIn('SDK_ROOT_MISSING_OR_INVALID', p.stderr)

    def test_manager_missing_fails_closed(self):
        p = self.run_helper()
        self.assertNotEqual(p.returncode, 0)
        self.assertIn('HOST_SDKMANAGER_UNAVAILABLE', p.stderr)

    def test_ndk_install_failure_never_reports_pass(self):
        self.manager()
        self.env['ZD_FIXTURE_INSTALL'] = 'no'
        p = self.run_helper()
        self.assertNotEqual(p.returncode, 0)
        self.assertIn('PINNED_NDK_INSTALL_FAILED', p.stderr)
        self.assertNotIn('ANDROID_NDK_HOME=', self.env_file.read_text())

    def test_mismatched_ndk_fails_closed(self):
        self.manager()
        self.env['ZD_FIXTURE_REVISION'] = '26.0.0'
        p = self.run_helper()
        self.assertNotEqual(p.returncode, 0)
        self.assertIn('PINNED_NDK_VERSION_MISMATCH', p.stderr)
        self.assertNotIn('ANDROID_NDK_HOME=', self.env_file.read_text())

    def test_git_env_symlink_refused(self):
        self.manager()
        destination = self.home/'some-other-file'
        destination.write_text('DO NOT MODIFY')
        self.env_file.unlink()
        self.env_file.symlink_to(destination)
        p = self.run_helper()
        self.assertNotEqual(p.returncode, 0)
        self.assertIn('GITHUB_ENV_UNAVAILABLE', p.stderr)
        self.assertEqual(destination.read_text(), 'DO NOT MODIFY')

    def test_workflow_uses_real_bootstrap_without_obsolete_action(self):
        flow = (ROOT/'.github/workflows/zygisk-detach-quality.yml').read_text()
        self.assertNotIn('android-actions/setup-android@v3', flow)
        self.assertIn('bash tools/ci_android_sdk.sh', flow)
        self.assertIn('needs: [rust-cli-and-config, webui, native-portable, module-package-unit]', flow)
        self.assertIn('actions/upload-artifact@v4', flow)
        self.assertIn('cargo build --manifest-path cli/Cargo.toml --release --target aarch64-linux-android', flow)
        self.assertIn('cargo build --manifest-path cli/Cargo.toml --release --target armv7-linux-androideabi', flow)

if __name__ == '__main__':
    unittest.main()
