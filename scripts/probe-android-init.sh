#!/bin/sh
set -eu
ulimit -c 0

if [ "$(id -u)" -ne 0 ] || [ -z "${SUDO_USER:-}" ]; then
    echo "Run through sudo from the repository root" >&2
    exit 1
fi

case "${DROIDPC_PROBE_HOST_CAPS:-0}" in
    0|1) ;;
    *) echo "DROIDPC_PROBE_HOST_CAPS must be 0 or 1" >&2; exit 1 ;;
esac
case "${DROIDPC_PROBE_WINDOW:-0}" in
    0|1) ;;
    *) echo "DROIDPC_PROBE_WINDOW must be 0 or 1" >&2; exit 1 ;;
esac

if [ "${DROIDPC_PROBE_BPF_MEMLOCK:-0}" = 1 ] && ! ulimit -l 1048576; then
    echo "Could not set the probe's locked-memory limit to 1 GiB" >&2
    exit 1
fi

if [ "${DROIDPC_PRIVATE_MOUNT_NS:-0}" != 1 ]; then
    export DROIDPC_PRIVATE_MOUNT_NS=1
    exec unshare --mount --propagation private "$0" "$@"
fi

root=$(pwd -P)
system=$(realpath "$root/os_images/selected/system.img")
vendor=$(realpath "$root/os_images/selected/vendor.img")
render_node=
drm_card_name=
if [ -c /dev/dri/renderD128 ]; then
    render_node=$(realpath /dev/dri/renderD128)
    render_device=$(readlink -f /sys/class/drm/renderD128/device 2>/dev/null || true)
    for candidate in /dev/dri/card*; do
        [ -c "$candidate" ] || continue
        candidate_name=${candidate##*/}
        candidate_device=$(readlink -f "/sys/class/drm/$candidate_name/device" 2>/dev/null || true)
        if [ -n "$render_device" ] && [ "$candidate_device" = "$render_device" ]; then
            drm_card_name=$candidate_name
            break
        fi
    done
fi
wayland_display=${WAYLAND_DISPLAY:-wayland-0}
wayland_socket=
guest_runtime_dir=/run/user/$(id -u "$SUDO_USER")
viewer_pid=
guest_pid=
monitor_pid=
input_fifo=
frame_file=
case "$wayland_display" in
    '') ;;
    /*)
        if [ -S "$wayland_display" ]; then
            wayland_socket=$wayland_display
        fi
        ;;
    */*) ;;
    *)
        runtime_dir=/run/user/$(id -u "$SUDO_USER")
        if [ -S "$runtime_dir/$wayland_display" ]; then
            wayland_socket=$runtime_dir/$wayland_display
        else
            for candidate in "$runtime_dir"/wayland-*; do
                if [ -S "$candidate" ]; then
                    wayland_socket=$candidate
                    wayland_display=${candidate##*/}
                    break
                fi
            done
        fi
        ;;
esac

if [ "${DROIDPC_PROBE_WINDOW:-0}" = 1 ]; then
    if [ "${DROIDPC_PROBE_HOST_CAPS:-0}" != 1 ] \
        || [ -z "$render_node" ] || [ -z "$wayland_socket" ]; then
        echo "Window mode requires host caps, DRM and a Wayland socket" >&2
        exit 1
    fi
    /usr/bin/python3 -c 'import gi; gi.require_version("Gtk", "3.0"); from gi.repository import Gtk, GdkPixbuf' || {
        echo "Window mode requires Python GTK 3 bindings" >&2
        exit 1
    }
    command -v runuser >/dev/null || {
        echo "Window mode requires runuser" >&2
        exit 1
    }
    command -v setsid >/dev/null || {
        echo "Window mode requires setsid" >&2
        exit 1
    }
    user_home=$(getent passwd "$SUDO_USER" | cut -d: -f6)
    case "$user_home" in
        /*) ;;
        *) echo "Could not resolve the Android data owner home" >&2; exit 1 ;;
    esac
    user_home=$(realpath "$user_home")
    data_parent=$(realpath -m "$user_home/.local/share/droidpc-probe")
    case "$data_parent/" in
        "$user_home/"*) ;;
        *) echo "Android data path resolves outside the user's home" >&2; exit 1 ;;
    esac
    # Preserve guest app state between foreground window sessions.
    data_dir=$data_parent/data
else
    data_dir=
fi
tmp=$(mktemp -d /tmp/droidpc-init.XXXXXX)
if [ -z "$data_dir" ]; then
    data_dir=$tmp/data
fi

cleanup() {
    if [ -n "$monitor_pid" ]; then
        kill -TERM "$monitor_pid" 2>/dev/null || true
        wait "$monitor_pid" 2>/dev/null || true
        monitor_pid=
    fi
    if [ -n "$guest_pid" ]; then
        kill -TERM "$guest_pid" 2>/dev/null || true
        wait "$guest_pid" 2>/dev/null || true
        guest_pid=
    fi
    if [ -n "$viewer_pid" ]; then
        kill -TERM -- "-$viewer_pid" 2>/dev/null || true
        wait "$viewer_pid" 2>/dev/null || true
        viewer_pid=
    fi
    umount "$tmp/vendor" 2>/dev/null || true
    umount "$tmp/system" 2>/dev/null || true
    umount "$tmp/dri" 2>/dev/null || true
    if [ -n "$input_fifo" ]; then
        rm -f -- "$input_fifo"
    fi
    if [ -n "$frame_file" ]; then
        rm -f -- "$frame_file" "$frame_file.tmp"
    fi
    rm -rf -- "$tmp"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$tmp/system" "$tmp/vendor" "$tmp/proc-sys/kernel"
mkdir -p "$tmp/sys" "$tmp/sys/fs"
if [ "${DROIDPC_PROBE_WINDOW:-0}" = 1 ]; then
    if [ ! -d "$data_dir" ]; then
        runuser -u "$SUDO_USER" -- mkdir -p "$data_dir"
    fi
    if [ ! -d "$data_dir" ] || [ -L "$data_dir" ]; then
        echo "Android data path is not a real directory" >&2
        exit 1
    fi
    data_dir=$(realpath "$data_dir")
    case "$data_dir/" in
        "$user_home/"*) ;;
        *) echo "Android data path resolves outside the user's home" >&2; exit 1 ;;
    esac
    frame_file=$data_dir/droidpc-window-$$.png
else
    mkdir -p "$data_dir"
fi
if [ "${DROIDPC_PROBE_WINDOW:-0}" = 1 ]; then
    attempt=0
    while [ "$attempt" -lt 1000 ]; do
        input_fifo=$guest_runtime_dir/droidpc-input-$$-$attempt.pipe
        if mkfifo "$input_fifo" 2>/dev/null; then
            break
        fi
        input_fifo=
        attempt=$((attempt + 1))
    done
    if [ -z "$input_fifo" ]; then
        echo "Could not create a unique Android input FIFO" >&2
        exit 1
    fi
    chown "$(id -u "$SUDO_USER"):$(id -g "$SUDO_USER")" "$input_fifo"
    chmod 0600 "$input_fifo"
fi
if [ -n "$render_node" ]; then
    mkdir "$tmp/dri"
    mount -t tmpfs -o mode=0755 tmpfs "$tmp/dri"
    prepare_drm_node() {
        node_name=$1
        device_numbers=$(cat "/sys/class/drm/$node_name/dev")
        node_major=${device_numbers%%:*}
        node_minor=${device_numbers#*:}
        case "$node_major" in
            ''|*[!0-9]*) return 1 ;;
        esac
        case "$node_minor" in
            ''|*[!0-9]*) return 1 ;;
        esac
        node_path=$tmp/dri/$node_name
        mknod "$node_path" c "$node_major" "$node_minor"
        # App processes lack AID_GRAPHICS; only expose these private guest nodes.
        chown 0:1003 "$node_path"
        chmod 0666 "$node_path"
        expected_device=$(printf '%x:%x' "$node_major" "$node_minor")
        if [ "$(stat -c '%t:%T' "$node_path")" != "$expected_device" ] \
            || [ "$(stat -c '%u:%g:%a' "$node_path")" != '0:1003:666' ]; then
            echo "Invalid staged DRM node: $node_name" >&2
            return 1
        fi
    }
    python3 "$root/scripts/stage-gpu-sysfs.py" "$tmp/gpu-sys" \
        renderD128 ${drm_card_name:+"$drm_card_name"}
    prepare_drm_node renderD128
    if [ -n "$drm_card_name" ]; then
        prepare_drm_node "$drm_card_name"
    fi
fi
touch "$tmp/kmsg"
debugfs -R 'cat /system/etc/ueventd.rc' "$system" 2>/dev/null \
    | sed 's/^uevent_socket_rcvbuf_size 16M$/uevent_socket_rcvbuf_size 256K/' \
    > "$tmp/ueventd.rc"
if ! grep -q '^uevent_socket_rcvbuf_size 256K$' "$tmp/ueventd.rc"; then
    echo "Could not prepare temporary ueventd configuration" >&2
    exit 1
fi
debugfs -R 'cat /system/etc/init/lmkd.rc' "$system" 2>/dev/null \
    > "$tmp/lmkd.rc"
if [ "${DROIDPC_PROBE_HOST_CAPS:-0}" != 1 ]; then
    sed 's/^    user lmkd$/    user root/' "$tmp/lmkd.rc" > "$tmp/lmkd-root.rc"
    mv "$tmp/lmkd-root.rc" "$tmp/lmkd.rc"
fi
if ! grep -q '^    user ' "$tmp/lmkd.rc"; then
    echo "Could not prepare temporary lmkd service configuration" >&2
    exit 1
fi
debugfs -R 'cat /system/etc/init/hw/init.zygote64_32.rc' "$system" 2>/dev/null \
    | sed '/^    priority -20$/d' > "$tmp/zygote.rc"
if ! grep -q '^service zygote ' "$tmp/zygote.rc"; then
    echo "Could not prepare temporary zygote service configuration" >&2
    exit 1
fi
cat > "$tmp/capture-logcat.sh" <<'SH'
#!/system/bin/sh
exec /system/bin/logcat -b all -v threadtime -r 1024 -n 1 \
    -f /data/droidpc-logcat
SH
cat > "$tmp/read-cgroup.sh" <<'SH'
#!/system/bin/sh
cat /sys/fs/cgroup/cgroup.controllers > /data/droidpc-controllers
cat /sys/fs/cgroup/cgroup.subtree_control > /data/droidpc-enabled
SH
cat > "$tmp/read-dev.sh" <<'SH'
#!/system/bin/sh
/system/bin/ls -l /dev > /data/droidpc-devices
/system/bin/ls -lZ /dev/dri > /data/droidpc-dri-devices 2>&1
SH
cat > "$tmp/read-drm.sh" <<'SH'
#!/system/bin/sh
report=$1
{
    id
    found=
    for node in /dev/dri/*; do
        [ -c "$node" ] || continue
        found=1
        if (exec 3<>"$node"); then
            echo "DRM_GRAPHICS_OPEN_OK=$node"
        else
            echo "DRM_GRAPHICS_OPEN_FAILED=$node"
        fi
    done
    [ -n "$found" ] || echo DRM_GRAPHICS_NODES_MISSING
} > "$report" 2>&1
SH
cat > "$tmp/read-binder.sh" <<'SH'
#!/system/bin/sh
{
    cat /proc/1/attr/current
    /system/bin/ls -lZ /dev/binder /dev/binderfs/binder
    /system/bin/ls -lZ /system/bin/logd /system/bin/boringssl_self_test32
    if (exec 3<>/dev/binderfs/binder); then
        echo BINDERFS_DEVICE_OPEN_OK
    else
        echo BINDERFS_DEVICE_OPEN_FAILED
    fi
    if (exec 4<>/dev/binder); then
        echo BINDER_LINK_OPEN_OK
    else
        echo BINDER_LINK_OPEN_FAILED
    fi
} > /data/droidpc-binder 2>&1
/system/bin/boringssl_self_test32 > /data/droidpc-boringssl 2>&1
echo "$?" > /data/droidpc-boringssl-status
SH
cat > "$tmp/read-profile.sh" <<'SH'
#!/system/bin/sh
cat /proc/1/cgroup > /data/droidpc-profile-cgroup
getprop ro.bootmode > /data/droidpc-bootmode
getprop ro.crypto.state > /data/droidpc-crypto-state
getprop ro.sf.lcd_density > /data/droidpc-lcd-density
getprop ro.hardware.egl > /data/droidpc-hardware-egl
getprop ro.hardware.gralloc > /data/droidpc-hardware-gralloc
getprop ro.hardware.vulkan > /data/droidpc-hardware-vulkan
getprop sys.use_memfd > /data/droidpc-use-memfd
vendor_namespace=$1
getprop "$vendor_namespace.stub_sensors_hal" > /data/droidpc-stub-sensors-hal
getprop "$vendor_namespace.host.uid" > /data/droidpc-host-uid
getprop "$vendor_namespace.host.gid" > /data/droidpc-host-gid
cat /proc/1/uid_map > /data/droidpc-uid-map
cat /proc/1/gid_map > /data/droidpc-gid-map
SH
cat > "$tmp/read-final-state.sh" <<'SH'
#!/system/bin/sh
report=/data/droidpc-final-state
echo 'BOOT_DIAGNOSTIC_STARTED' > "$report"
anr_captured=
capture_state() {
{
    echo "--- $1 boot/service snapshot ---"
    for property in sys.boot_completed init.svc.surfaceflinger init.svc.zygote \
        init.svc.bootanim init.svc.vendor.audio-hal \
        init.svc.vendor.audio-hal-2-0 init.svc.vendor.graphics.allocator-4-0 \
        init.svc.vendor.hwcomposer-2-1; do
        printf '%s=' "$property"
        getprop "$property"
    done
    echo '--- SystemServer process ---'
    server_pid=$(pidof system_server || true)
    ps -A | grep system_server || true
    echo '--- Framework service checks ---'
    /system/bin/service check activity 2>&1 || true
    /system/bin/service check package 2>&1 || true
    if [ -z "$anr_captured" ]; then
        anr=$(ls -t /data/anr/anr_* 2>/dev/null | head -n 1)
        if [ -n "$anr" ]; then
            echo "LATEST_ANR_TRACE=$anr"
        else
            echo 'LATEST_ANR_TRACE_MISSING'
        fi
    fi
    if [ -z "$server_pid" ]; then
        echo 'SYSTEM_SERVER_STACKS_UNAVAILABLE'
    fi
} >> "$report"
}
capture_state Initial
attempt=0
while [ "$attempt" -lt 18 ]; do
    boot_completed=$(getprop sys.boot_completed)
    if [ -z "$anr_captured" ]; then
        anr=$(ls -t /data/anr/anr_* 2>/dev/null | head -n 1)
        if [ -n "$anr" ]; then
            echo "LATEST_ANR_TRACE=$anr" >> "$report"
            anr_captured=1
        fi
    fi
    [ "$boot_completed" = 1 ] && break
    sleep 5
    attempt=$((attempt + 1))
done
server_pid=$(pidof system_server || true)
if [ "$(getprop sys.boot_completed)" != 1 ] && [ -n "$server_pid" ]; then
    if [ -x /system/bin/debuggerd ]; then
        if ! /system/bin/debuggerd -b "$server_pid" \
            > /data/droidpc-system-server-backtrace 2>&1; then
            echo 'debuggerd backtrace command failed' \
                >> /data/droidpc-system-server-backtrace
        fi
    else
        echo 'debuggerd is unavailable' \
            > /data/droidpc-system-server-backtrace
    fi
fi
capture_state Final
if [ "$(getprop sys.boot_completed)" = 1 ]; then
    if /system/bin/screencap -p /data/droidpc-screenshot.png; then
        echo 'SCREEN_CAPTURE_OK' >> "$report"
    else
        echo 'SCREEN_CAPTURE_FAILED' >> "$report"
    fi
else
    echo 'SCREEN_CAPTURE_SKIPPED_BOOT_INCOMPLETE' >> "$report"
fi
SH
if [ "${DROIDPC_PROBE_WINDOW:-0}" = 1 ]; then
    cat > "$tmp/capture-screen.sh" <<SH
#!/system/bin/sh
while true; do
    if /system/bin/screencap -p /data/${frame_file##*/}.tmp; then
        chmod 0644 /data/${frame_file##*/}.tmp
        mv -f /data/${frame_file##*/}.tmp /data/${frame_file##*/}
    fi
    sleep 0.2
done
SH
    cat > "$tmp/read-input.sh" <<'SH'
#!/system/bin/sh
while read -r action a b c d duration; do
    case "$action" in
        tap)
            case "$a:$b" in *[!0-9:]*|:*|*:) continue ;; esac
            /system/bin/input tap "$a" "$b"
            ;;
        swipe)
            case "$a:$b:$c:$d:$duration" in
                *[!0-9:]*|:*|*::*|*:) continue ;;
            esac
            /system/bin/input swipe "$a" "$b" "$c" "$d" "$duration"
            ;;
        key)
            case "$a" in ''|*[!0-9]*) continue ;; esac
            /system/bin/input keyevent "$a"
            ;;
    esac
done < /data/droidpc-input.pipe
SH
fi
/usr/bin/python3 "$root/scripts/prepare_android_vendor_properties.py" \
    "$vendor" "$SUDO_USER" "$(id -u "$SUDO_USER")" \
    "$(id -g "$SUDO_USER")" "$tmp/vendor-properties.prop" \
    > "$tmp/vendor-property-info"
vendor_namespace=$(sed -n '1p' "$tmp/vendor-property-info")
vendor_property_path=$(sed -n '2p' "$tmp/vendor-property-info")

printf '%s\n' \
    'on early-init' \
    '    write /data/droidpc-early-init PASS' \
    '    setprop sys.use_memfd true' \
    '    setprop init.svc_debug.no_fatal.lmkd true' \
    '    copy /proc/1/mounts /data/droidpc-mounts' \
    '    mkdir /dev/binderfs 0755 root root' \
    '    mount binder binder /dev/binderfs' \
    '    chmod 0666 /dev/binderfs/binder' \
    '    chmod 0666 /dev/binderfs/hwbinder' \
    '    chmod 0666 /dev/binderfs/vndbinder' \
    '    symlink /dev/binderfs/binder /dev/binder' \
    '    symlink /dev/binderfs/hwbinder /dev/hwbinder' \
    '    symlink /dev/binderfs/vndbinder /dev/vndbinder' \
    '    exec -- /system/bin/sh /tmp/droidpc-read-binder.sh' \
    '    exec -- /system/bin/sh /tmp/droidpc-read-cgroup.sh' \
    'on init' \
    '    write /data/droidpc-init-trigger PASS' \
    '    exec -- /system/bin/sh /tmp/droidpc-read-dev.sh' \
    '    write /data/droidpc-drm-access DRM_GRAPHICS_CHECK_NOT_RUN' \
    '    chown system graphics /data/droidpc-drm-access' \
    '    exec - system graphics -- /system/bin/sh /tmp/droidpc-read-drm.sh /data/droidpc-drm-access' \
    '    write /data/droidpc-drm-launcher-access DRM_GRAPHICS_CHECK_NOT_RUN' \
    '    chown 10102 10102 /data/droidpc-drm-launcher-access' \
    '    exec - 10102 10102 -- /system/bin/sh /tmp/droidpc-read-drm.sh /data/droidpc-drm-launcher-access' \
    '    write /data/droidpc-drm-systemui-access DRM_GRAPHICS_CHECK_NOT_RUN' \
    '    chown 10108 10108 /data/droidpc-drm-systemui-access' \
    '    exec - 10108 10108 -- /system/bin/sh /tmp/droidpc-read-drm.sh /data/droidpc-drm-systemui-access' \
    '    task_profiles HighPerformance' \
    "    exec -- /system/bin/sh /tmp/droidpc-read-profile.sh $vendor_namespace" \
    '    exec_background -- /system/bin/sh /tmp/droidpc-read-final-state.sh' \
    'on property:init.svc.logd=running' \
    '    exec_background -- /system/bin/sh /tmp/droidpc-capture-logcat.sh' \
    'on charger' \
    '    write /data/droidpc-charger-trigger PASS' \
    'on late-init' \
    '    write /data/droidpc-late-init-trigger PASS' \
    'on property:sys.boot_completed=1' \
    '    write /data/droidpc-boot-completed PASS' \
    > "$tmp/probe.rc"
if [ "${DROIDPC_PROBE_WINDOW:-0}" = 1 ]; then
    cat >> "$tmp/probe.rc" <<'RC'
on property:sys.boot_completed=1
    exec_background -- /system/bin/sh /tmp/droidpc-read-input.sh
RC
    cat >> "$tmp/probe.rc" <<'RC'
on property:sys.boot_completed=1
    exec_background -- /system/bin/sh /tmp/droidpc-capture-screen.sh
RC
fi

debugfs -R 'cat /system/etc/init/hw/init.rc' "$system" \
    2>/dev/null > "$tmp/init.rc"
if ! grep -q '^on late-init$' "$tmp/init.rc"; then
    echo "Could not read the image's primary init.rc" >&2
    exit 1
fi
cat "$tmp/probe.rc" >> "$tmp/init.rc"
debugfs -R 'cat /system/build.prop' "$system" 2>/dev/null \
    > "$tmp/system-build.prop"
if ! grep -q '^ro.sf.lcd_density=' "$tmp/system-build.prop"; then
    printf '%s\n' 'ro.sf.lcd_density=160' >> "$tmp/system-build.prop"
fi
sed -i '/^ro.hardware.egl=/d; /^ro.hardware.gralloc=/d; /^ro.hardware.vulkan=/d' \
    "$tmp/system-build.prop"
if [ -n "$render_node" ]; then
    printf '%s\n' \
        'ro.hardware.egl=mesa' \
        'ro.hardware.gralloc=minigbm_gbm_mesa' \
        'ro.hardware.vulkan=intel' >> "$tmp/system-build.prop"
else
    printf '%s\n' \
        'ro.hardware.egl=swiftshader' \
        'ro.hardware.gralloc=default' \
        'ro.hardware.vulkan=pastel' >> "$tmp/system-build.prop"
fi
printf '%s\n' 'androidboot.mode=normal' > "$tmp/cmdline"

mount -t ext4 -o loop,ro,norecovery "$system" "$tmp/system"
mount -t ext4 -o loop,ro,norecovery "$vendor" "$tmp/vendor"
printf '4\n' > "$tmp/proc-sys/kernel/kptr_restrict"

status=0
set -- /usr/bin/bwrap \
        --unshare-pid --unshare-ipc --unshare-uts \
        --unshare-net --unshare-cgroup \
        --die-with-parent --as-pid-1 \
        --clearenv --cap-add ALL
if [ "${DROIDPC_PROBE_HOST_CAPS:-0}" = 1 ]; then
    set -- "$@" \
        --cap-drop CAP_SYS_BOOT --cap-drop CAP_SYS_MODULE \
        --cap-drop CAP_SYS_RAWIO --cap-drop CAP_SYS_TIME \
        --cap-drop CAP_SYS_PTRACE
fi
set -- "$@" \
        --ro-bind "$tmp/system" / \
        --ro-bind "$tmp/init.rc" /system/etc/init/hw/init.rc \
        --ro-bind "$tmp/system-build.prop" /system/build.prop \
        --ro-bind "$tmp/lmkd.rc" /system/etc/init/lmkd.rc \
        --ro-bind "$tmp/zygote.rc" /system/etc/init/hw/init.zygote64_32.rc \
        --ro-bind "$tmp/ueventd.rc" /system/etc/ueventd.rc \
        --ro-bind "$tmp/vendor" /vendor \
        --ro-bind "$tmp/vendor-properties.prop" "$vendor_property_path" \
        --proc /proc --dev /dev \
        --dir /dev/socket --tmpfs /tmp
if [ -n "$render_node" ]; then
    # Regular binds/remount-ro add nodev; these are disposable guest nodes.
    set -- "$@" --dir /dev/dri --dev-bind "$tmp/dri" /dev/dri
fi
if [ "${DROIDPC_PROBE_WINDOW:-0}" = 1 ]; then
    set -- "$@" --ro-bind "$tmp/capture-screen.sh" \
        /tmp/droidpc-capture-screen.sh \
        --ro-bind "$tmp/read-input.sh" /tmp/droidpc-read-input.sh
fi
set -- "$@" \
        --ro-bind "$tmp/read-cgroup.sh" /tmp/droidpc-read-cgroup.sh \
        --ro-bind "$tmp/read-dev.sh" /tmp/droidpc-read-dev.sh \
        --ro-bind "$tmp/read-drm.sh" /tmp/droidpc-read-drm.sh \
        --ro-bind "$tmp/read-binder.sh" /tmp/droidpc-read-binder.sh \
        --ro-bind "$tmp/read-profile.sh" /tmp/droidpc-read-profile.sh \
        --ro-bind "$tmp/read-final-state.sh" /tmp/droidpc-read-final-state.sh \
        --ro-bind "$tmp/capture-logcat.sh" /tmp/droidpc-capture-logcat.sh \
        --ro-bind "$tmp/cmdline" /proc/cmdline \
        --bind "$tmp/proc-sys" /proc/sys \
        --bind "$tmp/sys" /sys \
        --bind "$tmp/kmsg" /dev/kmsg \
        --bind "$data_dir" /data --tmpfs /mnt \
        --tmpfs /apex \
        --tmpfs /linkerconfig
if [ "${DROIDPC_PROBE_WINDOW:-0}" = 1 ]; then
    set -- "$@" --bind "$input_fifo" /data/droidpc-input.pipe
fi
if [ -n "$render_node" ]; then
    for subtree in dev devices bus class; do
        set -- "$@" --ro-bind "$tmp/gpu-sys/$subtree" \
            "/sys/$subtree"
    done
fi
set -- "$@" --chdir / /system/bin/init second_stage
if [ "${DROIDPC_PROBE_HOST_CAPS:-0}" = 1 ]; then
    echo "WARNING: Android init has host-level capabilities inside private namespaces."
else
    set -- /usr/bin/unshare --user \
        --map-users=0:0:65536 \
        --map-groups=0:0:65536 \
        --setgroups=allow --setuid=0 --setgid=0 \
        "$@"
fi
if [ "${DROIDPC_PROBE_SELINUX_DOMAIN:-0}" = 1 ]; then
    set -- /usr/bin/runcon -t droidpc_probe_t -- "$@"
fi
if [ "${DROIDPC_PROBE_WINDOW:-0}" = 1 ]; then
    setsid runuser -u "$SUDO_USER" -- env \
        XDG_RUNTIME_DIR="$guest_runtime_dir" \
        WAYLAND_DISPLAY="$wayland_display" \
        /usr/bin/python3 "$root/scripts/android-window.py" \
        "$frame_file" "$input_fifo" \
        > "$tmp/window.log" 2>&1 &
    viewer_pid=$!
    sleep 1
    if ! kill -0 "$viewer_pid" 2>/dev/null; then
        cat "$tmp/window.log" >&2
        echo "Could not start the Android display window" >&2
        exit 1
    fi
    echo "Android window active; click to tap, drag to swipe, use keyboard navigation."
    echo "Guest data is stored at $data_dir."
    echo "Press Ctrl-C here to stop the guest."
    "$@" &
    guest_pid=$!
    (
        while kill -0 "$guest_pid" 2>/dev/null; do
            if ! kill -0 "$viewer_pid" 2>/dev/null; then
                echo "Android display process exited; stopping the guest." >&2
                tail -n 80 "$tmp/window.log" >&2
                kill -TERM "$guest_pid" 2>/dev/null || true
                break
            fi
            sleep 1
        done
    ) &
    monitor_pid=$!
    wait "$guest_pid" || status=$?
    guest_pid=
    kill -TERM "$monitor_pid" 2>/dev/null || true
    wait "$monitor_pid" 2>/dev/null || true
    monitor_pid=
else
    timeout --signal=TERM --kill-after=2s 180s "$@" || status=$?
fi

echo "INIT_PROBE_STATUS=$status"
if [ -n "$render_node" ]; then
    echo "HOST_RENDER_NODE_AVAILABLE"
else
    echo "HOST_RENDER_NODE_MISSING"
fi
if [ -n "$drm_card_name" ]; then
    echo "HOST_DRM_CARD_AVAILABLE"
    echo "HOST_DRM_CARD=$drm_card_name"
else
    echo "HOST_DRM_CARD_MISSING"
fi
if [ -n "$wayland_socket" ]; then
    echo "HOST_WAYLAND_SOCKET_MOUNTED"
else
    echo "HOST_WAYLAND_SOCKET_MISSING"
fi
if [ -s "$data_dir/droidpc-early-init" ]; then
    echo "INIT_EARLY_INIT_OK"
else
    echo "INIT_EARLY_INIT_MISSING"
fi
if [ -s "$data_dir/droidpc-init-trigger" ]; then
    echo "INIT_TRIGGER_OK"
else
    echo "INIT_TRIGGER_MISSING"
fi
if [ -s "$data_dir/droidpc-mounts" ]; then
    echo "--- Guest cgroup mounts ---"
    awk '$3 == "cgroup" || $3 == "cgroup2" { print }' \
        "$data_dir/droidpc-mounts"
else
    echo "GUEST_MOUNT_LIST_MISSING"
fi
for name in controllers enabled; do
    if [ -f "$data_dir/droidpc-$name" ]; then
        printf 'GUEST_CGROUP_%s=' "$name"
        cat "$data_dir/droidpc-$name"
        printf '\n'
    else
        echo "GUEST_CGROUP_${name}_MISSING"
    fi
done
if [ -f "$data_dir/droidpc-devices" ]; then
    echo "--- Guest /dev entries ---"
    cat "$data_dir/droidpc-devices"
    if [ -f "$data_dir/droidpc-dri-devices" ]; then
        echo "--- Guest DRM devices ---"
        cat "$data_dir/droidpc-dri-devices"
    fi
    if [ -f "$data_dir/droidpc-drm-access" ]; then
        echo "--- Android system user DRM access ---"
        cat "$data_dir/droidpc-drm-access"
    fi
    for app in launcher systemui; do
        if [ -f "$data_dir/droidpc-drm-$app-access" ]; then
            echo "--- Android $app DRM access ---"
            cat "$data_dir/droidpc-drm-$app-access"
        fi
    done
else
    echo "GUEST_DEVICE_LIST_MISSING"
fi
if [ -f "$data_dir/droidpc-binder" ]; then
    echo "--- Guest Binder access ---"
    cat "$data_dir/droidpc-binder"
else
    echo "GUEST_BINDER_CHECK_MISSING"
fi
if [ -f "$data_dir/droidpc-boringssl-status" ]; then
    printf 'BORINGSSL_SELF_TEST32_STATUS='
    cat "$data_dir/droidpc-boringssl-status"
    cat "$data_dir/droidpc-boringssl"
else
    echo "BORINGSSL_SELF_TEST32_DIAGNOSTIC_MISSING"
fi
if grep -q "Command 'task_profiles HighPerformance'.*failed" \
    "$tmp/kmsg"; then
    echo "TASK_PROFILE_ACTION_FAILED"
else
    echo "TASK_PROFILE_ACTION_NO_FAILURE_REPORTED"
fi
if [ -f "$data_dir/droidpc-profile-cgroup" ]; then
    printf 'GUEST_INIT_CGROUP='
    cat "$data_dir/droidpc-profile-cgroup"
else
    echo "GUEST_INIT_CGROUP_MISSING"
fi
for name in bootmode crypto-state; do
    label=$(printf '%s' "$name" | tr '[:lower:]' '[:upper:]')
    printf 'GUEST_%s=' "$label"
    if [ -f "$data_dir/droidpc-$name" ]; then
        cat "$data_dir/droidpc-$name"
    else
        echo "UNAVAILABLE"
    fi
done
for name in lcd-density hardware-egl hardware-gralloc hardware-vulkan \
    use-memfd stub-sensors-hal host-uid host-gid; do
    label=$(printf '%s' "$name" | tr '[:lower:]-' '[:upper:]_')
    printf 'GUEST_%s=' "$label"
    if [ -f "$data_dir/droidpc-$name" ]; then
        cat "$data_dir/droidpc-$name"
    else
        echo "UNAVAILABLE"
    fi
done
for name in uid-map gid-map; do
    printf 'GUEST_%s=' "$(printf '%s' "$name" | tr '[:lower:]-' '[:upper:]_')"
    if [ -f "$data_dir/droidpc-$name" ]; then
        tr '\n' ';' < "$data_dir/droidpc-$name"
        printf '\n'
    else
        echo UNAVAILABLE
    fi
done
if [ -f "$data_dir/droidpc-final-state" ]; then
    echo "--- Guest final boot/service state ---"
    cat "$data_dir/droidpc-final-state"
else
    echo "GUEST_FINAL_STATE_UNAVAILABLE"
fi
if [ -s "$data_dir/droidpc-screenshot.png" ]; then
    screenshot=$(mktemp --suffix=.png /tmp/droidpc-screen.XXXXXX)
    cp "$data_dir/droidpc-screenshot.png" "$screenshot"
    chown "$SUDO_USER" "$screenshot"
    chmod 0600 "$screenshot"
    echo "ANDROID_SCREENSHOT=$screenshot"
else
    echo "ANDROID_SCREENSHOT_MISSING"
fi
if [ -f "$data_dir/droidpc-system-server-backtrace" ]; then
    echo "--- Bounded SystemServer backtrace (first 160 lines) ---"
    sed -n '1,160p' "$data_dir/droidpc-system-server-backtrace"
fi
if [ -s "$data_dir/droidpc-charger-trigger" ]; then
    echo "CHARGER_TRIGGERED"
else
    echo "CHARGER_NOT_TRIGGERED"
fi
if [ -s "$data_dir/droidpc-late-init-trigger" ]; then
    echo "LATE_INIT_TRIGGERED"
else
    echo "LATE_INIT_NOT_TRIGGERED"
fi
if [ -s "$data_dir/droidpc-boot-completed" ]; then
    echo "ANDROID_BOOT_COMPLETED"
else
    echo "ANDROID_BOOT_NOT_COMPLETED"
fi
echo "--- Android init log (start) ---"
sed -n '1,80p' "$tmp/kmsg"
echo "--- Android init log (recent) ---"
tail -n 120 "$tmp/kmsg"
if [ -f "$data_dir/droidpc-logcat" ]; then
    echo "--- Android logcat failures ---"
    grep -E -i 'lmkd|lowmemorykiller|lowmemdetector|psi|memory pressure|bpfloader|zygote|system_server|systemserver|SystemServer|ActivityManager|AndroidRuntime|surfaceflinger|incidentd|fatal|crash|boot_completed|DEBUG|signal 11|hwcomposer|composer|gralloc|angle|egl|vulkan|wayland|wl_display' \
        "$data_dir/droidpc-logcat" | tail -n 180 || true
else
    echo "ANDROID_LOGCAT_MISSING"
fi
echo "--- Android init failures ---"
grep -n -E 'Permission denied|reboot_on_failure|shutting down|could not be opened|Could not open|vold|binder|Fatal|failed to mount|exited with status' \
    "$tmp/kmsg" | tail -n 100 || true
line=$(grep -n "Service boringssl_self_test32 has" "$tmp/kmsg" \
    | head -n 1 | cut -d: -f1)
if [ -n "$line" ]; then
    start=$((line > 16 ? line - 16 : 1))
    end=$((line + 4))
    echo "--- BoringSSL self-test failure context ---"
    sed -n "${start},${end}p" "$tmp/kmsg"
fi
if command -v ausearch >/dev/null 2>&1; then
    echo "--- Host SELinux denials ---"
    ausearch -m AVC,USER_AVC -ts recent -i 2>/dev/null \
        | grep -E 'binder|system/bin/(logd|boringssl|vold)|denied' \
        | tail -n 60 || true
fi
exit "$status"
