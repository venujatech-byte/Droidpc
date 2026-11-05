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
host_uid=$(id -u "$SUDO_USER")
host_gid=$(id -g "$SUDO_USER")
case "$host_uid:$host_gid:$uid_start:$gid_start" in
  *[!0-9:]*) echo "Invalid host or subordinate ID" >&2; exit 1 ;;
esac
if [ -z "$uid_start" ] || [ -z "$gid_start" ] \
  || [ "$host_uid" -lt 1 ] || [ "$host_uid" -gt 65535 ] \
  || [ "$host_gid" -lt 1 ] || [ "$host_gid" -gt 65535 ]; then
  echo "Valid host UID/GID and subordinate ranges of 65535 are required" >&2
  exit 1
fi

set -- unshare --user
for kind in users groups; do
  if [ "$kind" = users ]; then
    host_id=$host_uid
    subordinate_start=$uid_start
  else
    host_id=$host_gid
    subordinate_start=$gid_start
  fi
  [ "$host_id" -eq 0 ] || set -- "$@" \
    "--map-$kind=0:$subordinate_start:$host_id"
  set -- "$@" "--map-$kind=$host_id:$host_id:1"
  if [ "$host_id" -lt 65535 ]; then
    guest_after=$((host_id + 1))
    subordinate_after=$((subordinate_start + host_id))
    count_after=$((65535 - host_id))
    set -- "$@" \
      "--map-$kind=$guest_after:$subordinate_after:$count_after"
  fi
done
"$@" --setgroups=allow --setuid=0 --setgid=0 \
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
import sys

def covers_android_ids(path):
    with open(path, encoding="ascii") as mapping:
        return any(
            int(parts[0]) <= 1000 < int(parts[0]) + int(parts[2])
            for line in mapping
            if len(parts := line.split()) == 3
        )

assert covers_android_ids("/proc/self/uid_map")
assert covers_android_ids("/proc/self/gid_map")
host_uid, host_gid, uid_start, gid_start = map(int, sys.argv[1:])

def read_map(path):
    with open(path, encoding="ascii") as mapping:
        return [tuple(map(int, line.split())) for line in mapping]

def expected(host_id, subordinate_start):
    result = []
    if host_id:
        result.append((0, subordinate_start, host_id))
    result.append((host_id, host_id, 1))
    if host_id < 65535:
        result.append((host_id + 1, subordinate_start + host_id,
                       65535 - host_id))
    return result

assert read_map("/proc/self/uid_map") == expected(host_uid, uid_start)
assert read_map("/proc/self/gid_map") == expected(host_gid, gid_start)
os.setgroups([host_gid])
assert os.getgroups() == [host_gid]
print("PASS host UID/GID identity mapping, subordinate Android IDs, setgroups")
' "$host_uid" "$host_gid" "$uid_start" "$gid_start"
