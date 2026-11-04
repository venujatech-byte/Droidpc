#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ] || [ -z "${SUDO_USER:-}" ]; then
    echo "Run through sudo from the repository root" >&2
    exit 1
fi

root=$(pwd -P)
module=droidpc_probe
tmp=$(mktemp -d /tmp/droidpc-selinux.XXXXXX)
installed=0

cleanup() {
    if [ "$installed" -eq 1 ]; then
        semodule -r "$module" || echo "Remove temporary policy with: sudo semodule -r $module" >&2
    fi
    rm -rf -- "$tmp"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

checkmodule -M -m -o "$tmp/$module.mod" "$root/scripts/droidpc-probe.te"
semodule_package -o "$tmp/$module.pp" -m "$tmp/$module.mod"
semodule -i "$tmp/$module.pp"
installed=1

DROIDPC_PROBE_SELINUX_DOMAIN=1 DROIDPC_PROBE_BPF_MEMLOCK=1 \
    "$root/scripts/probe-android-init.sh"
