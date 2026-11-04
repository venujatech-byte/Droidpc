#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ] || [ -z "${SUDO_USER:-}" ]; then
  echo "Run this probe through sudo from your normal user" >&2
  exit 1
fi

find_range() {
  awk -F: -v name="$SUDO_USER" '
    $1 == name && $2 ~ /^[0-9]+$/ && $3 ~ /^[0-9]+$/ &&
    $3 >= 65535 { print $2; exit }
  ' "$1"
}

uid_start=$(find_range /etc/subuid)
gid_start=$(find_range /etc/subgid)
if [ -z "$uid_start" ] || [ -z "$gid_start" ]; then
  echo "A subordinate UID and GID range of 65535 is required" >&2
  exit 1
fi

unshare --user \
  --map-users=0:0:1 \
  --map-users="1:$uid_start:65535" \
  --map-groups=0:0:1 \
  --map-groups="1:$gid_start:65535" \
  --setgroups=allow \
  --setuid=0 --setgid=0 \
  /usr/bin/bwrap \
    --unshare-pid --unshare-ipc --unshare-uts \
    --unshare-net --unshare-cgroup \
    --die-with-parent --clearenv \
    --cap-add ALL \
    --ro-bind /usr /usr \
    --ro-bind /lib64 /lib64 \
    --proc /proc --dev /dev \
  /usr/bin/python3 -c '
import os

def covers_android_ids(path):
    with open(path, encoding="ascii") as mapping:
        return any(
            int(parts[0]) <= 1000 < int(parts[0]) + int(parts[2])
            for line in mapping
            if len(parts := line.split()) == 3
        )

assert covers_android_ids("/proc/self/uid_map")
assert covers_android_ids("/proc/self/gid_map")
os.setgroups([1000])
assert os.getgroups() == [1000]
print("PASS Bubblewrap UID 1000, GID 1000, and setgroups")
'
