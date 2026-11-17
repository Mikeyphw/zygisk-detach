#!/system/bin/sh
# Read-only acceptance evidence; does NOT change installed modules or packages.
set -u
module=/data/adb/modules/zygisk-detach
if [ ! -d "$module" ]; then
    echo "ZD_DEVICE_REFUSED MODULE_NOT_INSTALLED"
    exit 2
fi
if [ ! -f "$module/detach" ]; then
    echo "ZD_DEVICE_REFUSED CLI_MISSING"
    exit 2
fi
printf '%s\n' '===== Android properties ====='
getprop ro.build.version.release
getprop ro.build.version.sdk
getprop ro.product.cpu.abilist
printf '%s\n' '===== Root/module evidence ====='
ls -ld "$module" /data/adb/zygisk-detach 2>&1 || true
printf '%s\n' '===== Configuration and observed hook evidence ====='
"$module/detach" doctor --json
printf '%s\n' '===== No mutation performed. Observe Play Store behavior separately. ====='
