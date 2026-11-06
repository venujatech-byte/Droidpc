"""The verifier succeeds only on both checks from the current app run."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class DeviceCheckTests(unittest.TestCase):
    def test_requires_current_run_https_and_camera_results(self):
        source = (Path(__file__).resolve().parents[1] / 'diagnose-android-graphics.sh').read_text()
        helper = source.split('45s "$@" /system/bin/sh -c \'', 1)[1].split("\n    ' sh", 1)[0]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            commands = root / 'commands'
            commands.mkdir()
            scripts = {'getprop': 'echo 1', 'pm': 'echo Success', 'am': 'exit 0',
                       'logcat': 'cat "$CHECK_LOG"', 'sleep': 'exit 0'}
            for name, script in scripts.items():
                path = commands / name
                path.write_text('#!/bin/sh\n' + script + '\n')
                path.chmod(0o700)
            helper = helper.replace('/data/local/tmp', str(root / 'apk')).replace('/data/droidpc-device-check.log', str(root / 'report'))
            fixture = root / 'log'
            for lines, expected in [
                ('run-1 HTTPS_OK 204\nrun-1 CAMERA_FRAME_OK 640x480 bytes=100\nrun-1 CHECK_COMPLETE\n', 0),
                ('run-1 HTTPS_OK 204\nrun-1 CAMERA_FAIL no devices\nrun-1 CHECK_COMPLETE\n', 1),
                ('old-run HTTPS_OK 204\nold-run CAMERA_FRAME_OK 640x480\nold-run CHECK_COMPLETE\n', 1),
            ]:
                with self.subTest(log=lines):
                    fixture.write_text(lines)
                    result = subprocess.run(['sh', '-c', helper, 'check', 'run-1'], input=b'test apk',
                                            env=os.environ | {'PATH': str(commands) + os.pathsep + os.environ['PATH'], 'CHECK_LOG': str(fixture)}, capture_output=True, timeout=5)
                    self.assertEqual(result.returncode, expected, result.stderr)
                    self.assertFalse((root / 'apk/droidpc-device-check.apk').exists())
