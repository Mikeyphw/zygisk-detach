#!/usr/bin/env bash
# The hosted ubuntu-latest runner already supplies Android command-line tools.
# Never ask sdkmanager for the removed legacy `tools` package.
set -euo pipefail
refuse() { printf 'ZD_ANDROID_SDK_REFUSED %s\n' "$1" >&2; exit 2; }

sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
[[ -n "$sdk_root" && "$sdk_root" == /* && -d "$sdk_root" ]] || refuse 'SDK_ROOT_MISSING_OR_INVALID'
sdkmanager="$sdk_root/cmdline-tools/latest/bin/sdkmanager"
[[ -x "$sdkmanager" && ! -L "$sdkmanager" ]] || refuse 'HOST_SDKMANAGER_UNAVAILABLE'
[[ -n "${GITHUB_ENV:-}" && -f "$GITHUB_ENV" && ! -L "$GITHUB_ENV" ]] || refuse 'GITHUB_ENV_UNAVAILABLE'
"$sdkmanager" --version >/dev/null || refuse 'SDKMANAGER_VERSION_CHECK_FAILED'

ndk_version='27.2.12479018'
ndk_home="$sdk_root/ndk/$ndk_version"
if [[ ! -x "$ndk_home/ndk-build" ]]; then
    "$sdkmanager" "--sdk_root=$sdk_root" --install "ndk;$ndk_version" || refuse 'PINNED_NDK_INSTALL_FAILED'
fi
[[ -x "$ndk_home/ndk-build" ]] || refuse 'PINNED_NDK_BINARY_MISSING'
[[ -f "$ndk_home/source.properties" ]] || refuse 'PINNED_NDK_PROPERTIES_MISSING'
grep -Eq '^Pkg.Revision[[:space:]]*=[[:space:]]*27\.2\.12479018[[:space:]]*$' "$ndk_home/source.properties" || refuse 'PINNED_NDK_VERSION_MISMATCH'

printf 'ANDROID_SDK_ROOT=%s\n' "$sdk_root" >> "$GITHUB_ENV"
printf 'ANDROID_NDK_HOME=%s\n' "$ndk_home" >> "$GITHUB_ENV"
printf 'ZD_ANDROID_SDK_PASS NDK=%s\n' "$ndk_version"
