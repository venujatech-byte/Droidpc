#!/usr/bin/env python3
"""Run a fixed shell check from the verified image in a disposable sandbox."""

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


SYSTEM_SHA256 = (
    '30c87ee44412fee4f50173716a6bb7c6941d3021853f0adab4a249da8a9455ab'
)
RUNTIME = 'system/apex/com.android.runtime'
FILES = [
    'system/bin/sh',
    f'{RUNTIME}/bin/linker64',
    f'{RUNTIME}/lib64/bionic/libc.so',
    f'{RUNTIME}/lib64/bionic/libdl.so',
]
CHECK = r'''
set -eu
[ ! -e /home ]
[ ! -e /sys ]
[ ! -e /vendor ]
if (echo x > /write-test) 2>/dev/null; then
    exit 10
fi
caps=
privs=
while read -r key value; do
    case "$key" in
        CapEff:) caps=$value ;;
        NoNewPrivs:) privs=$value ;;
    esac
done < /proc/self/status
[ "$caps" = "0000000000000000" ]
[ "$privs" = "1" ]
echo 'PASS Android shell, read-only root, no host home/sys/vendor'
echo 'PASS zero effective capabilities and no-new-privileges'
echo 'DROIDPC_SHELL_OK'
'''


def extract(image, root):
    for index, source in enumerate(FILES):
        # Numeric destinations avoid interpreting paths as debugfs commands.
        temporary = root / str(index)
        result = subprocess.run(
            ['debugfs', '-R', f'dump /{source} {index}',
             f'/proc/self/fd/{image.fileno()}'],
            cwd=root, pass_fds=(image.fileno(),), capture_output=True,
            text=True, timeout=10, check=True,
        )
        if not temporary.is_file() or temporary.stat().st_size == 0:
            raise RuntimeError(f'Cannot extract {source}: {result.stderr}')
        with temporary.open('rb') as binary:
            if binary.read(4) != b'\x7fELF':
                raise RuntimeError(f'Not an ELF file: {source}')
        destination = root / source
        destination.parent.mkdir(parents=True, exist_ok=True)
        temporary.rename(destination)
        destination.chmod(0o755 if '/bin/' in source else 0o644)
    for name in ['apex', 'proc', 'dev']:
        (root / name).mkdir()
    (root / 'apex/com.android.runtime').symlink_to(f'../{RUNTIME}')
    (root / 'system/bin/linker64').symlink_to(
        '/apex/com.android.runtime/bin/linker64'
    )


def sandbox(root, command):
    return subprocess.run(
        ['bwrap', '--unshare-user', '--unshare-pid', '--unshare-net',
         '--unshare-ipc', '--unshare-uts', '--unshare-cgroup',
         '--die-with-parent', '--new-session', '--cap-drop', 'ALL',
         '--ro-bind', str(root), '/', '--proc', '/proc',
         '--remount-ro', '/proc', '--dev', '/dev', '--clearenv',
         '--setenv', 'PATH', '/system/bin',
         '--setenv', 'LD_LIBRARY_PATH',
         '/apex/com.android.runtime/lib64/bionic',
         '--chdir', '/', '--', '/system/bin/sh', '-c', command],
        stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=10,
    )


def probe(path):
    if os.geteuid() == 0:
        raise RuntimeError('Run without sudo')
    if not path.is_file():
        raise RuntimeError('Expected a regular system image file')
    for tool in ['debugfs', 'bwrap']:
        if shutil.which(tool) is None:
            raise RuntimeError(f'Required developer tool not found: {tool}')
    with path.open('rb') as image:
        if hashlib.file_digest(image, 'sha256').hexdigest() != SYSTEM_SHA256:
            raise RuntimeError('Image hash differs from the inspected release')
        print('PASS verified system image hash', flush=True)
        with tempfile.TemporaryDirectory(prefix='droidpc-shell-') as temp:
            root = Path(temp)
            extract(image, root)
            result = sandbox(root, CHECK)
            passed = 'DROIDPC_SHELL_OK\n' in result.stdout
            if result.returncode != 0 or not passed:
                raise RuntimeError(
                    f'Guest check failed ({result.returncode}): '
                    f'{result.stdout}{result.stderr}'
                )
            print(result.stdout, end='')
            if result.stderr:
                print(result.stderr, end='', file=sys.stderr)
            failure = sandbox(root, 'exit 7')
            if failure.returncode != 7:
                raise RuntimeError(f'Guest exit status lost: {failure}')
            print('PASS guest failure exit status propagated')
        if root.exists():
            raise RuntimeError(f'Temporary root was not removed: {root}')
        print('PASS temporary root removed')


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('Usage: python3 scripts/probe-shell.py SYSTEM_IMAGE')
    try:
        probe(Path(sys.argv[1]).resolve(strict=True))
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        raise SystemExit(f'FAIL: {error}') from error
