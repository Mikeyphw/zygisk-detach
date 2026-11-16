#!/system/bin/sh

chmod -R +x "$MODPATH/bin/"

if [ -n "${KSU:-}" ]; then
    ui_print "- KernelSU detected. Zygisk must be enabled and visible in Play Store."
    ui_print "- Root app profiles are left untouched; check module injection in your manager."
fi

if [ ! -f "$MODPATH/bin/$ARCH/detach" ]; then
    ui_print "ERROR: missing ABI-specific detach executable"; return 1
fi
mv -f "$MODPATH/bin/$ARCH/detach" "$MODPATH/detach"
mkdir -p /data/adb/zygisk-detach/

DBIN="/data/adb/zygisk-detach/detach.bin"
if [ -f "$MODPATH/detach.txt" ]; then
	ui_print "- detach.txt inside module: generating detach.bin"
	OP=$("$MODPATH"/detach serialize "$MODPATH/detach.txt" $DBIN 2>&1)
	ui_print "$OP"
elif [ -f "$MODPATH/detach.bin" ]; then
	ui_print "- detach.bin inside module: applying"
    if [ -e "$DBIN" ] || [ -L "$DBIN" ]; then
        ui_print "- Existing configuration preserved; bundled detach.bin not applied"
    else
        mv "$MODPATH/detach.bin" "$DBIN"
        chmod 600 "$DBIN"
    fi
fi

CLIPATH=/data/data/com.termux/files/usr/bin/
if [ -d $CLIPATH ]; then
	if [ ! -e "$CLIPATH/detach" ] && [ ! -L "$CLIPATH/detach" ]; then
        (umask 022; printf '%s\n' '#!/data/data/com.termux/files/usr/bin/sh' 'exec su 0 /data/adb/modules/zygisk-detach/detach "$@"' > "$CLIPATH/detach")
        chmod 755 "$CLIPATH/detach"
        ui_print "- Installed safe Termux CLI wrapper"
    else
        ui_print "- Existing Termux detach command preserved"
    fi
else
	ui_print "- Install termux to use the 'detach' cli"
fi

ui_print "- Or use zygisk-detach-app"
if [ -n "$KSU" ]; then
	ui_print "- Or use the WebUI"
fi
ui_print ""
ui_print "  by j-hc (github.com/j-hc)"
