#!/usr/bin/env python3
"""Check a temporary systemd delegation; never run as root or directly."""

import os
from pathlib import Path


def probe():
    if os.geteuid() == 0:
        raise RuntimeError("Run through the user systemd service, without sudo")
    entries = Path('/proc/self/cgroup').read_text().splitlines()
    membership = next((row[3:] for row in entries if row.startswith('0::')), '')
    group = Path(membership)
    if not group.is_absolute() or '..' in group.parts:
        raise RuntimeError('No usable cgroup v2 membership')
    if group.name != 'droidpc-cgroup-probe.service':
        raise RuntimeError('Run inside droidpc-cgroup-probe.service')

    # This developer probe targets the host mount verified by doctor.
    root = Path('/sys/fs/cgroup') / membership.lstrip('/')
    processes = (root / 'cgroup.procs').read_text().split()
    if processes != [str(os.getpid())]:
        raise RuntimeError('Probe service must contain only this process')
    required = {'cpu', 'memory', 'pids'}
    available = set((root / 'cgroup.controllers').read_text().split())
    if not required <= available:
        missing = required - available
        raise RuntimeError(f'Missing delegated controllers: {missing}')

    manager = root / 'manager'
    guest = root / 'guest'
    manager.mkdir()
    # Keep the delegated parent empty before enabling domain controllers.
    (manager / 'cgroup.procs').write_text(str(os.getpid()))
    (root / 'cgroup.subtree_control').write_text('+cpu +memory +pids')
    guest.mkdir()
    try:
        limits = {
            'memory.max': '33554432',
            'pids.max': '16',
            'cpu.max': '20000 100000',
        }
        for name, value in limits.items():
            control = guest / name
            control.write_text(value)
            observed = control.read_text().strip()
            if observed != value:
                raise RuntimeError(f'{name}: expected {value}, got {observed}')
            print(f'PASS {name} = {observed}')
        if (guest / 'cgroup.procs').read_text().strip():
            raise RuntimeError('Unexpected process in the empty guest cgroup')
    finally:
        guest.rmdir()
    print('PASS empty guest cgroup removed')
    print('Limit configuration verified; workload enforcement not tested.')
    # systemd removes the manager leaf and service after this process exits.


if __name__ == '__main__':
    try:
        probe()
    except (OSError, RuntimeError) as error:
        raise SystemExit(f'FAIL: {error}') from error
