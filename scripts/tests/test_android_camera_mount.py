"""Camera staging must not inherit the host temporary directory's nodev flag."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class CameraMountTests(unittest.TestCase):
    def test_mount_allows_devices_before_nodes_are_created(self):
        source = (Path(__file__).resolve().parents[1] / 'probe-android-init.sh').read_text()
        start = source.index('if [ "${DROIDPC_PROBE_WINDOW:-0}" = 1 ]; then\n    mkdir "$tmp/camera"')
        end = source.index('touch "$tmp/kmsg"', start)
        helper = source[start:end].replace('/dev/video[0-9]* /dev/media[0-9]*', '/dev/null')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            commands = root / 'bin'
            commands.mkdir()
            for name, script in {
                'mount': 'printf "mount %s\\n" "$*" >> "$CHECK_LOG"',
                'mknod': 'printf "mknod %s\\n" "$*" >> "$CHECK_LOG"; touch "$1"',
                'chown': 'printf "chown %s\\n" "$*" >> "$CHECK_LOG"',
            }.items():
                command = commands / name
                command.write_text('#!/bin/sh\n' + script + '\n')
                command.chmod(0o700)
            log = root / 'events'
            subprocess.run(['sh', '-eu', '-c', helper], env=os.environ | {
                'tmp': directory, 'DROIDPC_PROBE_WINDOW': '1', 'camera_nodes': '',
                'PATH': str(commands) + os.pathsep + os.environ['PATH'], 'CHECK_LOG': str(log),
            }, check=True)
            events = log.read_text().splitlines()
            self.assertTrue(events[0].startswith('mount -t tmpfs -o mode=0755,nosuid,noexec,dev '))
            self.assertTrue(events[1].startswith('mknod '))
            self.assertIn('chown 0:1006 ', events[2])
            self.assertEqual((root / 'camera/null').stat().st_mode & 0o777, 0o660)
        self.assertIn('umount "$tmp/camera" 2>/dev/null || true', source)
