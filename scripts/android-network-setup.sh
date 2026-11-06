#!/system/bin/sh
set -eu
# Mount from inside the guest network namespace: these views must never come from the host.
mkdir -p /tmp/droidpc-network-sys /tmp/droidpc-network-proc
mount -t sysfs -o ro,nosuid,nodev,noexec sysfs /tmp/droidpc-network-sys
mount -t proc -o nosuid,nodev,noexec proc /tmp/droidpc-network-proc
mkdir -p /sys/class/net /sys/devices/virtual/net /proc/sys/net
mount -o bind /tmp/droidpc-network-sys/class/net /sys/class/net
mount -o bind /tmp/droidpc-network-sys/devices/virtual/net /sys/devices/virtual/net
mount -o bind /tmp/droidpc-network-proc/sys/net /proc/sys/net
umount /tmp/droidpc-network-sys
umount /tmp/droidpc-network-proc

test -d /sys/class/net/lo
test -r /proc/sys/net/ipv4/ip_forward
