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
    umount "$tmp/binderfs" 2>/dev/null || true
    rm -rf -- "$tmp"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$tmp/system" "$tmp/vendor" "$tmp/proc-sys/kernel"
mkdir -p "$tmp/sys" "$tmp/sys/fs" "$tmp/data" "$tmp/binderfs"
touch "$tmp/kmsg"
debugfs -R 'cat /system/etc/ueventd.rc' "$system" 2>/dev/null \
    | sed 's/^uevent_socket_rcvbuf_size 16M$/uevent_socket_rcvbuf_size 256K/' \
    > "$tmp/ueventd.rc"
if ! grep -q '^uevent_socket_rcvbuf_size 256K$' "$tmp/ueventd.rc"; then
    echo "Could not prepare temporary ueventd configuration" >&2
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
    '    symlink /dev/binderfs/binder /dev/binder' \
    '    symlink /dev/binderfs/hwbinder /dev/hwbinder' \
    '    symlink /dev/binderfs/vndbinder /dev/vndbinder' \
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
mount -t binder binder "$tmp/binderfs"
for device in binder hwbinder vndbinder; do
    if [ ! -c "$tmp/binderfs/$device" ]; then
        echo "Binderfs did not create $device" >&2
        exit 1
    fi
    chmod 0666 "$tmp/binderfs/$device"
done
printf '4\n' > "$tmp/proc-sys/kernel/kptr_restrict"

uid_start=$(awk -F: -v name="$SUDO_USER" \
    '$1 == name && $3 >= 65535 { print $2; exit }' /etc/subuid)
gid_start=$(awk -F: -v name="$SUDO_USER" \
    '$1 == name && $3 >= 65535 { print $2; exit }' /etc/subgid)
if [ -z "$uid_start" ] || [ -z "$gid_start" ]; then
    echo "Missing subordinate UID/GID range" >&2
    exit 1
fi

status=0
timeout --signal=TERM --kill-after=2s 45s \
unshare --user \
    --map-users=0:0:1 \
    --map-users="1:$uid_start:65535" \
    --map-groups=0:0:1 \
    --map-groups="1:$gid_start:65535" \
    --setgroups=allow --setuid=0 --setgid=0 \
    /usr/bin/bwrap \
        --unshare-pid --unshare-ipc --unshare-uts \
        --unshare-net --unshare-cgroup \
        --die-with-parent --as-pid-1 \
        --clearenv --cap-add ALL \
        --ro-bind "$tmp/system" / \
        --ro-bind "$tmp/init.rc" /system/etc/init/hw/init.rc \
        --ro-bind "$tmp/ueventd.rc" /system/etc/ueventd.rc \
        --ro-bind "$tmp/vendor" /vendor \
        --proc /proc --dev /dev \
        --dir /dev/socket \
        --dir /dev/binderfs \
        --bind "$tmp/binderfs" /dev/binderfs \
        --tmpfs /tmp \
        --ro-bind "$tmp/read-cgroup.sh" /tmp/droidpc-read-cgroup.sh \
        --ro-bind "$tmp/read-dev.sh" /tmp/droidpc-read-dev.sh \
        --ro-bind "$tmp/read-profile.sh" /tmp/droidpc-read-profile.sh \
        --ro-bind "$tmp/cmdline" /proc/cmdline \
        --bind "$tmp/proc-sys" /proc/sys \
        --bind "$tmp/sys" /sys \
        --bind "$tmp/kmsg" /dev/kmsg \
        --bind "$tmp/data" /data --tmpfs /mnt \
        --tmpfs /apex \
        --tmpfs /linkerconfig \
        --setenv LD_LIBRARY_PATH \
        /system/apex/com.android.runtime/lib64/bionic:/system/lib64 \
        --chdir / \
        /system/bin/init second_stage || status=$?

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
printf 'GUEST_BOOTMODE='
cat "$tmp/data/droidpc-bootmode"
printf 'GUEST_CRYPTO_STATE='
cat "$tmp/data/droidpc-crypto-state"
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
exit "$status"
