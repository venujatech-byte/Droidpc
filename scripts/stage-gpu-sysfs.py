#!/usr/bin/env python3
"""Snapshot PCI GPU discovery metadata; never copy writable device controls."""
import os
import re
import sys
from pathlib import Path


def stage(source, target, names):
    source = source.resolve(strict=True)
    if not names or len(set(names)) != len(names):
        raise ValueError('Expected distinct DRM node names')
    nodes = []
    for name in names:
        if not re.fullmatch(r'(renderD|card)[0-9]+', name):
            raise ValueError('Invalid DRM node name')
        node = (source / 'class/drm' / name).resolve(strict=True)
        node.relative_to(source / 'devices')
        device = (node / 'device').resolve(strict=True)
        device.relative_to(source / 'devices')
        if (device / 'subsystem').resolve(strict=True) != source / 'bus/pci':
            raise ValueError('Only PCI GPU discovery is supported')
        numbers = (node / 'dev').read_text().strip()
        if not re.fullmatch(r'[0-9]+:[0-9]+', numbers):
            raise ValueError('Invalid DRM device number')
        nodes.append((name, node, device, numbers))
    if len({device for _, _, device, _ in nodes}) != 1:
        raise ValueError('DRM nodes belong to different GPUs')
    target.mkdir(mode=0o755)

    def link(path, destination):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.symlink_to(os.path.relpath(destination, path.parent))

    device = nodes[0][2]
    gpu = target / device.relative_to(source)
    gpu.mkdir(parents=True)
    # shortcut: immutable PCI discovery snapshot, refresh on each probe launch.
    for attribute in ('vendor', 'device', 'subsystem_vendor',
                      'subsystem_device', 'revision', 'class', 'uevent'):
        (gpu / attribute).write_bytes((device / attribute).read_bytes())
    (target / 'bus/pci').mkdir(parents=True)
    link(gpu / 'subsystem', target / 'bus/pci')
    link(target / 'bus/pci/devices' / device.name, gpu)
    for name, node, _, numbers in nodes:
        staged = target / node.relative_to(source)
        staged.mkdir(parents=True)
        for attribute in ('dev', 'uevent'):
            (staged / attribute).write_bytes((node / attribute).read_bytes())
        link(staged / 'device', gpu)
        link(target / 'class/drm' / name, staged)
        link(target / 'dev/char' / numbers, staged)


if __name__ == '__main__':
    stage(Path('/sys'), Path(sys.argv[1]), sys.argv[2:])
