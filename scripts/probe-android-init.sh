#!/bin/sh
set -eu
ulimit -c 0

if [ "$(id -u)" -ne 0 ] || [ -z "${SUDO_USER:-}" ]; then
    echo "Run through sudo from the repository root" >&2
    exit 1
fi

if [ "${DROIDPC_PRIVATE_MOUNT_NS:-0}" != 1 ]; then
    export DROIDPC_PRIVATE_MOUNT_NS=1
    exec unshare --mount --propagation private "$0" "$@"
fi

root=$(pwd -P)
system=$(realpath "$root/os_images/selected/system.img")
vendor=$(realpath "$root/os_images/selected/vendor.img")
tmp=$(mktemp -d /tmp/droidpc-init.XXXXXX)

cleanup() {
    umount "$tmp/vendor" 2>/dev/null || true
    umount "$tmp/system" 2>/dev/null || true
    rm -rf -- "$tmp"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$tmp/system" "$tmp/vendor" "$tmp/proc-sys/kernel"
mkdir -p "$tmp/sys" "$tmp/sys/fs" "$tmp/data"
touch "$tmp/kmsg"
debugfs -R 'cat /system/etc/ueventd.rc' "$system" 2>/dev/null \
    | sed 's/^uevent_socket_rcvbuf_size 16M$/uevent_socket_rcvbuf_size 256K/' \
    > "$tmp/ueventd.rc"
if ! grep -q '^uevent_socket_rcvbuf_size 256K$' "$tmp/ueventd.rc"; then
    echo "Could not prepare temporary ueventd configuration" >&2
    exit 1
fi
debugfs -R 'cat /system/etc/init/lmkd.rc' "$system" 2>/dev/null \
    | sed 's/^    user lmkd$/    user root/' > "$tmp/lmkd.rc"
if ! grep -q '^    user root$' "$tmp/lmkd.rc"; then
    echo "Could not prepare temporary lmkd service configuration" >&2
    exit 1
fi
cat > "$tmp/read-cgroup.sh" <<'SH'
#!/system/bin/sh
cat /sys/fs/cgroup/cgroup.controllers > /data/droidpc-controllers
cat /sys/fs/cgroup/cgroup.subtree_control > /data/droidpc-enabled
SH
cat > "$tmp/read-dev.sh" <<'SH'
#!/system/bin/sh
/system/bin/ls -l /dev > /data/droidpc-devices
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
SH
printf '%s\n' \
    'on early-init' \
    '    write /data/droidpc-early-init PASS' \
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
    '    task_profiles HighPerformance' \
    '    exec -- /system/bin/sh /tmp/droidpc-read-profile.sh' \
    'on charger' \
    '    write /data/droidpc-charger-trigger PASS' \
    'on late-init' \
    '    write /data/droidpc-late-init-trigger PASS' \
    'on property:sys.boot_completed=1' \
    '    write /data/droidpc-boot-completed PASS' \
    > "$tmp/probe.rc"

debugfs -R 'cat /system/etc/init/hw/init.rc' "$system" \
    2>/dev/null > "$tmp/init.rc"
if ! grep -q '^on late-init$' "$tmp/init.rc"; then
    echo "Could not read the image's primary init.rc" >&2
    exit 1
fi
cat "$tmp/probe.rc" >> "$tmp/init.rc"
printf '%s\n' 'androidboot.mode=normal' > "$tmp/cmdline"

mount -t ext4 -o loop,ro,norecovery "$system" "$tmp/system"
mount -t ext4 -o loop,ro,norecovery "$vendor" "$tmp/vendor"
printf '4\n' > "$tmp/proc-sys/kernel/kptr_restrict"

status=0
set -- /usr/bin/unshare --user \
    --map-users=0:0:65536 \
    --map-groups=0:0:65536 \
    --setgroups=allow --setuid=0 --setgid=0 \
    /usr/bin/bwrap \
        --unshare-pid --unshare-ipc --unshare-uts \
        --unshare-net --unshare-cgroup \
        --die-with-parent --as-pid-1 \
        --clearenv --cap-add ALL \
        --ro-bind "$tmp/system" / \
        --ro-bind "$tmp/init.rc" /system/etc/init/hw/init.rc \
        --ro-bind "$tmp/lmkd.rc" /system/etc/init/lmkd.rc \
        --ro-bind "$tmp/ueventd.rc" /system/etc/ueventd.rc \
        --ro-bind "$tmp/vendor" /vendor \
        --proc /proc --dev /dev \
        --dir /dev/socket \
        --tmpfs /tmp \
        --ro-bind "$tmp/read-cgroup.sh" /tmp/droidpc-read-cgroup.sh \
        --ro-bind "$tmp/read-dev.sh" /tmp/droidpc-read-dev.sh \
        --ro-bind "$tmp/read-binder.sh" /tmp/droidpc-read-binder.sh \
        --ro-bind "$tmp/read-profile.sh" /tmp/droidpc-read-profile.sh \
        --ro-bind "$tmp/cmdline" /proc/cmdline \
        --bind "$tmp/proc-sys" /proc/sys \
        --bind "$tmp/sys" /sys \
        --bind "$tmp/kmsg" /dev/kmsg \
        --bind "$tmp/data" /data --tmpfs /mnt \
        --tmpfs /apex \
        --tmpfs /linkerconfig \
        --chdir / \
        /system/bin/init second_stage
if [ "${DROIDPC_PROBE_SELINUX_DOMAIN:-0}" = 1 ]; then
    set -- /usr/bin/runcon -t droidpc_probe_t -- "$@"
fi
timeout --signal=TERM --kill-after=2s 45s "$@" || status=$?

echo "INIT_PROBE_STATUS=$status"
if [ -s "$tmp/data/droidpc-early-init" ]; then
    echo "INIT_EARLY_INIT_OK"
else
    echo "INIT_EARLY_INIT_MISSING"
fi
if [ -s "$tmp/data/droidpc-init-trigger" ]; then
    echo "INIT_TRIGGER_OK"
else
    echo "INIT_TRIGGER_MISSING"
fi
if [ -s "$tmp/data/droidpc-mounts" ]; then
    echo "--- Guest cgroup mounts ---"
    awk '$3 == "cgroup" || $3 == "cgroup2" { print }' \
        "$tmp/data/droidpc-mounts"
else
    echo "GUEST_MOUNT_LIST_MISSING"
fi
for name in controllers enabled; do
    if [ -f "$tmp/data/droidpc-$name" ]; then
        printf 'GUEST_CGROUP_%s=' "$name"
        cat "$tmp/data/droidpc-$name"
        printf '\n'
    else
        echo "GUEST_CGROUP_${name}_MISSING"
    fi
done
if [ -f "$tmp/data/droidpc-devices" ]; then
    echo "--- Guest /dev entries ---"
    cat "$tmp/data/droidpc-devices"
else
    echo "GUEST_DEVICE_LIST_MISSING"
fi
if [ -f "$tmp/data/droidpc-binder" ]; then
    echo "--- Guest Binder access ---"
    cat "$tmp/data/droidpc-binder"
else
    echo "GUEST_BINDER_CHECK_MISSING"
fi
if [ -f "$tmp/data/droidpc-boringssl-status" ]; then
    printf 'BORINGSSL_SELF_TEST32_STATUS='
    cat "$tmp/data/droidpc-boringssl-status"
    cat "$tmp/data/droidpc-boringssl"
else
    echo "BORINGSSL_SELF_TEST32_DIAGNOSTIC_MISSING"
fi
if grep -q "Command 'task_profiles HighPerformance'.*failed" \
    "$tmp/kmsg"; then
    echo "TASK_PROFILE_ACTION_FAILED"
else
    echo "TASK_PROFILE_ACTION_NO_FAILURE_REPORTED"
fi
if [ -f "$tmp/data/droidpc-profile-cgroup" ]; then
    printf 'GUEST_INIT_CGROUP='
    cat "$tmp/data/droidpc-profile-cgroup"
else
    echo "GUEST_INIT_CGROUP_MISSING"
fi
for name in bootmode crypto-state; do
    label=$(printf '%s' "$name" | tr '[:lower:]' '[:upper:]')
    printf 'GUEST_%s=' "$label"
    if [ -f "$tmp/data/droidpc-$name" ]; then
        cat "$tmp/data/droidpc-$name"
    else
        echo "UNAVAILABLE"
    fi
done
if [ -s "$tmp/data/droidpc-charger-trigger" ]; then
    echo "CHARGER_TRIGGERED"
else
    echo "CHARGER_NOT_TRIGGERED"
fi
if [ -s "$tmp/data/droidpc-late-init-trigger" ]; then
    echo "LATE_INIT_TRIGGERED"
else
    echo "LATE_INIT_NOT_TRIGGERED"
fi
if [ -s "$tmp/data/droidpc-boot-completed" ]; then
    echo "ANDROID_BOOT_COMPLETED"
else
    echo "ANDROID_BOOT_NOT_COMPLETED"
fi
echo "--- Android init log (start) ---"
sed -n '1,80p' "$tmp/kmsg"
echo "--- Android init log (recent) ---"
tail -n 120 "$tmp/kmsg"
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
