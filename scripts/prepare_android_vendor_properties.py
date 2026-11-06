#!/usr/bin/python3
"""Create host-specific properties using the guest image's own namespace."""
import re
import subprocess
import sys
from pathlib import Path


NAME = re.compile(r"[A-Za-z0-9_.-]+\Z")
ROOT_PROPERTY = re.compile(
    r"^\s*\d+\s+100\d+\s+\(\d+\).*?\s([A-Za-z0-9_.-]+\.prop)\s*$"
)


def debugfs(image, command):
    result = subprocess.run(
        ["debugfs", "-R", command, str(image)],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode:
        raise ValueError(result.stderr.strip() or "debugfs failed")
    return result.stdout


def vendor_brand(contents):
    values = [
        line.split("=", 1)[1]
        for line in contents.splitlines()
        if line.startswith("ro.product.vendor.brand=")
    ]
    if len(values) != 1 or not NAME.fullmatch(values[0]):
        raise ValueError("vendor image has no usable vendor brand namespace")
    return values[0]


def property_filename(listing, read_file):
    candidates = []
    for line in listing.splitlines():
        match = ROOT_PROPERTY.fullmatch(line)
        if not match:
            continue
        name = match.group(1)
        contents = read_file(name)
        values = [
            row for row in contents.splitlines()
            if row.strip() and not row.lstrip().startswith("#")
        ]
        if not values:
            candidates.append(name)
    if len(candidates) != 1:
        raise ValueError(
            "vendor image must have exactly one empty root property file"
        )
    return candidates[0]


def prepare(image, user, uid, gid):
    if not NAME.fullmatch(user) or not uid.isdecimal() or not gid.isdecimal():
        raise ValueError("invalid host account values")
    listing = debugfs(image, "ls -l /")
    brand = vendor_brand(debugfs(image, "cat /build.prop"))
    filename = property_filename(
        listing,
        lambda name: debugfs(image, f"cat /{name}"),
    )
    properties = "\n".join(
        (
            f"{brand}.host.user={user}",
            f"{brand}.host.uid={uid}",
            f"{brand}.host.gid={gid}",
            f"{brand}.stub_sensors_hal=1",
            "",
        )
    )
    return brand, f"/vendor/{filename}", properties


def main(args):
    if len(args) != 5:
        raise SystemExit(
            "usage: prepare_android_vendor_properties.py "
            "IMAGE USER UID GID OUTPUT"
        )
    image, user, uid, gid, output = args
    try:
        brand, guest_path, properties = prepare(image, user, uid, gid)
        Path(output).write_text(properties)
    except (OSError, ValueError) as error:
        print(f"Could not prepare guest vendor properties: {error}", file=sys.stderr)
        return 1
    print(brand)
    print(guest_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
