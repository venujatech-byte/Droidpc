#!/bin/sh
set -eu
ulimit -c 0

if [ "$(id -u)" -ne 0 ] || [ -z "${SUDO_USER:-}" ]; then
    echo "Run through sudo from the repository root" >&2
    exit 1
fi

root=$(pwd -P)
system=$(realpath "$root/os_images/selected/system.img")
vendor=$(realpath "$root/os_images/selected/vendor.img")
tmp=$(mktemp -d /tmp/droidpc-init.XXXXXX)
mkdir -p "$tmp/system" "$tmp/vendor" "$tmp/proc-sys/kernel"
mkdir -p "$tmp/sys" "$tmp/sys/fs" "$tmp/data"
touch "$tmp/kmsg"
printf '%s\n' 'androidboot.init_rc=/tmp/droidpc-probe.rc' > "$tmp/cmdline"
printf '%s\n' 'on early-init' '    write /data/droidpc-early-init PASS' \
    > "$tmp/probe.rc"

cleanup() {
    umount "$tmp/vendor" 2>/dev/null || true
    umount "$tmp/system" 2>/dev/null || true
    rm -rf -- "$tmp"
}
trap cleanup EXIT HUP INT TERM

mount -t ext4 -o loop,ro,norecovery "$system" "$tmp/system"
mount -t ext4 -o loop,ro,norecovery "$vendor" "$tmp/vendor"
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
timeout --signal=TERM --kill-after=2s 12s \
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
        --ro-bind "$tmp/vendor" /vendor \
        --proc /proc --dev /dev \
        --dir /dev/socket \
        --tmpfs /tmp \
        --ro-bind "$tmp/probe.rc" /tmp/droidpc-probe.rc \
        --ro-bind "$tmp/cmdline" /proc/cmdline \
        --bind "$tmp/proc-sys" /proc/sys \
        --bind "$tmp/sys" /sys \
        --bind "$tmp/kmsg" /dev/kmsg \
        --bind "$tmp/data" /data --tmpfs /mnt \
        --tmpfs /apex --dir /apex/com.android.runtime \
        --ro-bind "$tmp/system/system/apex/com.android.runtime" \
        /apex/com.android.runtime \
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
echo "--- Android init log ---"
sed -n '1,160p' "$tmp/kmsg"
exit "$status"
