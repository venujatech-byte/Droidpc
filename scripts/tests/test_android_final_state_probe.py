"""Keep useful boot state if the bounded Android probe is stopped early."""
import os
import subprocess
import tempfile
import time
import unittest
from pathlib import Path


class FinalStateProbeTests(unittest.TestCase):
    def test_snapshot_is_written_before_boot_wait(self):
        source = (Path(__file__).resolve().parents[1]
                  / 'probe-android-init.sh').read_text()
        helper = source.split(
            'cat > "$tmp/read-final-state.sh" <<\'SH\'\n', 1
        )[1].split('\nSH\n', 1)[0]

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            data = root / 'data'
            data.mkdir()
            commands = root / 'bin'
            commands.mkdir()
            script = root / 'probe.sh'
            script.write_text(helper.replace('/data', str(data)).replace(
                '/system/bin/service', 'service'))
            report = data / 'droidpc-final-state'
            state = root / 'boot-property-read'
            stubs = {
                'getprop': '''#!/bin/sh
if [ "$1" = sys.boot_completed ]; then
  if [ -e "$BOOT_PROPERTY_READ" ]; then sleep 30
  else touch "$BOOT_PROPERTY_READ"; echo 0; fi
else echo running; fi
''',
                'pidof': '#!/bin/sh\necho 42\n',
                'ps': '#!/bin/sh\necho system_server 42\n',
                'service': '#!/bin/sh\necho "Service $2: found"\n',
            }
            for name, contents in stubs.items():
                path = commands / name
                path.write_text(contents)
                path.chmod(0o755)

            environment = os.environ | {
                'PATH': f'{commands}:{os.environ["PATH"]}',
                'BOOT_PROPERTY_READ': str(state),
            }
            process = subprocess.Popen(['/bin/sh', str(script)],
                                       env=environment)
            try:
                deadline = time.monotonic() + 3
                while time.monotonic() < deadline:
                    if (report.exists()
                            and 'Service package: found' in report.read_text()):
                        break
                    time.sleep(0.01)
                self.assertTrue(report.exists())
                contents = report.read_text()
                self.assertIn('--- Initial boot/service snapshot ---', contents)
                self.assertIn('Service activity: found', contents)
                self.assertIn('Service package: found', contents)
                snapshots = contents.count(
                    '--- Initial boot/service snapshot ---')
                self.assertEqual(snapshots, 1)
            finally:
                process.terminate()
                process.wait(timeout=3)


if __name__ == '__main__':
    unittest.main()
