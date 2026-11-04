#!/usr/bin/env python3
"""Check guest cgroup placement inside one delegated user service."""

import os
from pathlib import Path
import sys


UNIT = 'droidpc-cgroup-placement.service'
CONTROLLERS = {'cpu', 'memory', 'pids'}


def membership():
    lines = Path('/proc/self/cgroup').read_text().splitlines()
    row = next((line for line in lines if line.startswith('0::')), '')
    path = Path(row[3:])
    if not path.is_absolute() or '..' in path.parts:
        raise RuntimeError('No safe cgroup v2 membership')
    return path


def child(guest):
    if membership() != Path('/'):
        raise RuntimeError('Guest cgroup namespace root is not /')
    if guest.name != 'guest' or guest.parent.name != UNIT:
        raise RuntimeError('Unexpected guest cgroup path')
    leaf = guest / 'init'
    leaf.mkdir()
    (leaf / 'cgroup.procs').write_text(str(os.getpid()))
    if membership() != Path('/init'):
        raise RuntimeError('Init process is not in its child leaf')
    if (guest / 'cgroup.procs').read_text().strip():
        raise RuntimeError('Guest parent still contains a process')
    (guest / 'cgroup.subtree_control').write_text(
        '+cpu +memory +pids'
    )
    enabled = set(
        (guest / 'cgroup.subtree_control').read_text().split()
    )
    if not CONTROLLERS <= enabled:
        raise RuntimeError(f'Controllers not enabled: {enabled}')
    print('PASS guest namespace root and init leaf', flush=True)
    print('PASS guest child controllers: cpu memory pids', flush=True)


def parent():
    if os.geteuid() == 0:
        raise RuntimeError('Run as the normal user, without sudo')
    group = membership()
    if group.name != UNIT:
        raise RuntimeError(f'Run inside {UNIT}')
    root = Path('/sys/fs/cgroup') / group.relative_to('/')
    if (root / 'cgroup.procs').read_text().split() != [str(os.getpid())]:
        raise RuntimeError('The service must contain only this process')
    available = set((root / 'cgroup.controllers').read_text().split())
    if not CONTROLLERS <= available:
        raise RuntimeError(f'Missing controllers: {CONTROLLERS - available}')

    manager = root / 'manager'
    guest = root / 'guest'
    manager.mkdir()
    (manager / 'cgroup.procs').write_text(str(os.getpid()))
    (root / 'cgroup.subtree_control').write_text(
        '+cpu +memory +pids'
    )
    guest.mkdir()
    try:
        pid = os.fork()
        if pid == 0:
            (guest / 'cgroup.procs').write_text(str(os.getpid()))
            os.execv('/usr/bin/unshare', [
                'unshare', '--user', '--map-auto',
                '--map-root-user', '--cgroup',
                '/usr/bin/python3', str(Path(__file__).resolve()),
                'child', str(guest),
            ])
        _, status = os.waitpid(pid, 0)
        if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 0:
            raise RuntimeError(f'Guest child failed: status {status}')
    finally:
        if (guest / 'init').exists():
            (guest / 'init').rmdir()
        guest.rmdir()
    print('PASS temporary guest cgroup removed')


if __name__ == '__main__':
    try:
        if len(sys.argv) == 1:
            parent()
        elif len(sys.argv) == 3 and sys.argv[1] == 'child':
            child(Path(sys.argv[2]))
        else:
            raise RuntimeError('Invalid probe arguments')
    except (OSError, RuntimeError) as error:
        raise SystemExit(f'FAIL: {error}') from error
