#!/bin/sh
# Non-root host CLI acceptance against disposable state only; DO NOT run on Android.
set -eu
cli=$1
case "$(uname -s)" in Linux) ;; *) echo 'host Linux only' >&2; exit 2;; esac
[ -f "$cli" ] || { echo 'missing host CLI binary' >&2;exit 2; }
if [ -n "${ANDROID_ROOT:-}" ] || command -v getprop >/dev/null 2>&1;then
 echo 'refusing host smoke test on Android' >&2;exit 2
fi
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
cd "$tmp"
"$cli" reset
"$cli" list >actual
[ ! -s actual ] || { echo 'reset left nonempty config' >&2; exit 1; }
"$cli" detach com.example.one com.example.two
[ -s detach.bin ] || { echo 'relative configuration was not persisted' >&2; exit 1; }
[ -d backups ] || { echo 'relative configuration backups directory missing' >&2; exit 1; }
printf '%s\n' com.example.one com.example.two >expected
"$cli" list >actual
diff -u expected actual
"$cli" detach com.example.one
"$cli" list >actual
diff -u expected actual
"$cli" reattach com.example.one
[ -s detach.bin ] || { echo 'reattach lost configuration' >&2; exit 1; }
printf '%s\n' com.example.two >expected
"$cli" list >actual
diff -u expected actual
"$cli" detachall com.example.third
printf '%s\n' com.example.third >expected
"$cli" list >actual
diff -u expected actual
"$cli" backups | grep -q '^0: backup-'
"$cli" restore 0
printf '%s\n' com.example.two >expected
"$cli" list >actual
diff -u expected actual
"$cli" doctor --json >diagnostics
python3 -c 'import json; x=json.load(open("diagnostics")); assert x["hook_state"] != "observed_in_current_store_process"'
"$cli" doctor --bundle evidence.json
[ -s evidence.json ]
if "$cli" doctor --bundle evidence.json;then echo 'bundle overwrite incorrectly allowed' >&2;exit 1;fi
python3 -c 'import json; x=json.load(open("evidence.json")); assert "hook_state" in x and "package_names" not in x'
if "$cli" verify;then echo 'unverified hook unexpectedly accepted' >&2;exit 1;fi
if "$cli" detach 'com.bad;touch';then echo 'invalid package accepted' >&2;exit 1;fi
printf '%s\n' 'com.example.imported' >import.txt
"$cli" import import.txt
printf '%s\n' com.example.imported >expected
"$cli" list >actual
diff -u expected actual
printf 'CORRUPT' >detach.bin
if "$cli" reset;then echo 'corrupt configuration reset unexpectedly' >&2;exit 1;fi
[ "$(cat detach.bin)" = 'CORRUPT' ]
"$cli" backups | grep -q '^0: backup-'
"$cli" restore 0
"$cli" list >/dev/null
printf 'ZD_CLI_SMOKE_PASS\n'
