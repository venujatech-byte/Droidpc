#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo 'Run: sudo ./scripts/diagnose-android-graphics.sh' >&2
    exit 1
fi

devices=0
[ "${1:-}" != --devices ] || devices=1
found=
for entry in /proc/[0-9]*/cmdline; do
    [ -r "$entry" ] || continue
    command=$(tr '\000' ' ' < "$entry" 2>/dev/null) || continue
    case "$command" in
        '/system/bin/init second_stage '*) ;;
        *) continue ;;
    esac
    process=${entry%/cmdline}
    # Identify our guest, without entering another Android runtime.
    [ -f "$process/root/tmp/droidpc-read-final-state.sh" ] || continue
    if [ -n "$found" ]; then
        echo 'Multiple Droidpc guests are running; close older sessions first.' >&2
        exit 1
    fi
    found=${process##*/}
done
if [ -z "$found" ]; then
    echo 'No running Droidpc guest found. Leave the Android window open and retry.' >&2
    exit 1
fi

printf 'DROIDPC_GUEST_HOST_PID=%s\n' "$found"
guest=/proc/$found/root
set -- nsenter --target "$found" --mount --pid --net --root --wdns=/ --
if command -v selinuxenabled >/dev/null 2>&1 && selinuxenabled; then
    set -- runcon -t droidpc_probe_t -- "$@"
fi

if [ "$devices" = 1 ]; then
    printf '\n--- Camera provider and network service state ---\n'
    timeout --signal=TERM --kill-after=2s 20s "$@" \
        /system/bin/sh -c '
            for service in netd vendor.camera-provider-2-7-ext cameraserver; do
                echo "$service=$(getprop init.svc.$service)"
            done
            ls -ln /dev/video* /dev/media* 2>/dev/null
            ip -brief address
            ip route
            echo "boot_completed=$(getprop sys.boot_completed)"
            ls -ld /sys/class/net /proc/sys/net /sys/fs/bpf
            service list | sed -n "1,12p"
            ping -c 1 -W 2 1.1.1.1
            ping -c 1 -W 2 example.com
            dumpsys connectivity | sed -n "/Active default network:/p; /Current Networks:/,+12p"
            wm size
            wm density
            dumpsys ethernet
        ' 2>&1 | head -100 || true
    printf '\n--- Camera and network errors ---\n'
    for log in "$guest/data/droidpc-logcat.1" "$guest/data/droidpc-logcat"; do
        [ -f "$log" ] || continue
        grep -Ei 'camera.*(denied|fail|error|device|external)|external.*camera|libnetd_updatable_init|BpfHandler|XfrmController|EthernetTracker|IpClient|DhcpClient' "$log" | tail -70 || true
    done
    exit 0
fi
printf '\n--- Boot state ---\n'
cat "$guest/data/droidpc-final-state" 2>/dev/null || true
printf '\n--- Graphics startup messages (repeated service lookups excluded) ---\n'
for log in "$guest/data/droidpc-logcat.1" "$guest/data/droidpc-logcat"; do
    [ -f "$log" ] || continue
    grep -Ei ' (SurfaceFlinger|hwcomposer|EGL-MAIN|MESA-LOADER|RenderEngine|HWC2|HWComposer)[[:space:]]*:|Fatal signal|Abort message' "$log" | tail -80 || true
done
printf '\n--- Init graphics messages ---\n'
grep -Ei 'surfaceflinger|hwcomposer|graphics.composer|droidpc-screen' "$guest/dev/kmsg" | tail -40 || true
for name in surfaceflinger android.hardware.graphics.composer@2.1-service; do
    printf '\n--- Native stacks: %s ---\n' "$name"
    timeout --signal=TERM --kill-after=2s 15s nsenter --target "$found" --mount --pid --root --wdns=/ -- \
        /system/bin/sh -c '
            pid=$(pidof "$1")
            if [ -z "$pid" ]; then echo "Process not running: $1"; exit 1; fi
            exec /system/bin/debuggerd -b "$pid"
        ' sh "$name" 2>&1 | head -180 || true
done
