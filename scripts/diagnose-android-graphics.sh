#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo 'Run: sudo ./scripts/diagnose-android-graphics.sh' >&2
    exit 1
fi

mode=${1:-graphics}
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
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

if [ "$mode" = --camera-access ]; then
    timeout --kill-after=2s 10s "$@" /system/bin/sh -c '
        ls -lnZ /dev/video* /dev/media* 2>/dev/null
        for pid in $(pidof android.hardware.camera.provider@2.7-external-service); do
            echo "CAMERA_PROVIDER_PID=$pid"
            grep -E "^(Name|Uid|Gid|Groups|CapEff|CapBnd):" /proc/$pid/status
            cat /proc/$pid/attr/current
        done
        id
        for node in /dev/video*; do
            if (exec 3<>"$node"); then echo "ROOT_OPEN_OK=$node"; fi
        done
    '
    exit 0
fi

if [ "$mode" = --verify ]; then
    if [ ! -f "$root/target/droidpc-device-check.apk" ] || [ "$root/scripts/android/devicecheck/DeviceCheck.java" -nt "$root/target/droidpc-device-check.apk" ] || [ "$root/scripts/android/devicecheck/AndroidManifest.xml" -nt "$root/target/droidpc-device-check.apk" ]; then
        if [ -z "${SUDO_USER:-}" ]; then
            echo 'Build the device check as your desktop user: ./scripts/build-android-device-check.sh' >&2
            exit 1
        fi
        runuser -u "$SUDO_USER" -- "$root/scripts/build-android-device-check.sh"
    fi
    token=droidpc-check-$$-$(date +%s)
    report=$(mktemp /tmp/droidpc-device-check.XXXXXX)
    trap 'rm -f -- "$report"' EXIT
    timeout --signal=TERM --kill-after=2s 45s "$@" /system/bin/sh -c '
        [ "$(getprop sys.boot_completed)" = 1 ] || { echo "Android has not finished booting"; exit 1; }
        mkdir -p /data/local/tmp
        cat > /data/local/tmp/droidpc-device-check.apk
        pm install -r -g /data/local/tmp/droidpc-device-check.apk || exit 1
        rm -f /data/local/tmp/droidpc-device-check.apk
        am force-stop com.droidpc.devicecheck
        am start -W -n com.droidpc.devicecheck/.DeviceCheck --es run "$1" || exit 1
        count=0
        while [ "$count" -lt 28 ]; do
            logcat -d -v brief -s DroidpcDeviceCheck:I "*:S" | grep -F "$1" > /data/droidpc-device-check.log
            grep -q CHECK_COMPLETE /data/droidpc-device-check.log && break
            sleep 1
            count=$((count + 1))
        done
        cat /data/droidpc-device-check.log
        am force-stop com.droidpc.devicecheck
        grep -q HTTPS_OK /data/droidpc-device-check.log && grep -q CAMERA_FRAME_OK /data/droidpc-device-check.log
    ' sh "$token" < "$root/target/droidpc-device-check.apk" > "$report" 2>&1 && result=0 || result=$?
    cat "$report"
    if [ "$result" -ne 0 ]; then
        echo 'Device verification failed; collecting camera initialization errors.'
        for device_log in "$guest/data/droidpc-devices.log.1" "$guest/data/droidpc-devices.log"; do
            [ ! -f "$device_log" ] || tail -50 "$device_log"
        done
        timeout --kill-after=2s 5s "$@" /system/bin/sh -c '
            logcat -d -v threadtime -s "CamPrvdr@2.4-external:V" "CamPrvdr@2.7-external:V" "ExtCamDev@3.4:V" "ExtCamDev@3.5:V" "CameraProviderManager:V" "*:S" | tail -80
            dumpsys media.camera | head -60
        ' || true
    fi
    exit "$result"
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
    for log in "$guest/data/droidpc-devices.log.1" "$guest/data/droidpc-devices.log" "$guest/data/droidpc-logcat.1" "$guest/data/droidpc-logcat"; do
        [ -f "$log" ] || continue
        grep -Ei 'CamPrvdr|ExtCam|CameraProviderManager|v4l2|camera.*(denied|fail|error|device|external)|external.*camera|libnetd_updatable_init|BpfHandler|XfrmController|EthernetTracker|IpClient|DhcpClient' "$log" | tail -70 || true
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
