"""Keep useful boot state if the bounded Android probe is stopped early."""
import os
import subprocess
import tempfile
import time
import unittest
from pathlib import Path


class FinalStateProbeTests(unittest.TestCase):
    def test_vendor_namespace_is_set_before_init_configuration_uses_it(self):
        source = (Path(__file__).resolve().parents[1]
                  / 'probe-android-init.sh').read_text()

        self.assertLess(
            source.index('vendor_namespace=$(sed'),
            source.index('read-profile.sh $vendor_namespace'),
        )

    def test_guest_receives_host_wayland_socket_for_graphics_composer(self):
        source = (Path(__file__).resolve().parents[1]
                  / 'probe-android-init.sh').read_text()

        self.assertIn('"$wayland_socket" "$guest_runtime_dir/$wayland_display"',
                      source)
        self.assertIn('--setenv WAYLAND_DISPLAY "$wayland_display"', source)

    def test_window_input_fifo_collision_retries_without_removing_existing_path(self):
        source = (Path(__file__).resolve().parents[1]
                  / 'probe-android-init.sh').read_text()
        allocator = source.split('    attempt=0\n', 1)[1].split(
            '    chown ', 1
        )[0].replace('$$', 'test')

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            collision = root / 'droidpc-input-test-0.pipe'
            collision.write_text('keep')
            script = root / 'allocate.sh'
            script.write_text(
                f'guest_runtime_dir="{root}"\nattempt=0\n{allocator}'
                'printf "%s\\n%s\\n" "$attempt" "$input_fifo"\n'
            )
            result = subprocess.run(['/bin/sh', str(script)],
                                    check=True, capture_output=True,
                                    text=True, timeout=3)
            attempt, input_path = result.stdout.splitlines()

            self.assertEqual(attempt, '1')
            self.assertTrue(collision.is_file())
            self.assertTrue(Path(input_path).is_fifo())

    def test_window_guest_input_helper_accepts_only_numeric_commands(self):
        source = (Path(__file__).resolve().parents[1]
                  / 'probe-android-init.sh').read_text()
        helper = source.split(
            'cat > "$tmp/read-input.sh" <<\'SH\'\n', 1
        )[1].split('\nSH\n', 1)[0]

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            commands = root / 'bin'
            commands.mkdir()
            input_tool = commands / 'input'
            input_tool.write_text('''#!/bin/sh
printf '%s\\n' "$*" >> "$INPUT_LOG"
''')
            input_tool.chmod(0o755)
            injection_marker = root / 'injection-marker'
            input_file = root / 'commands'
            input_file.write_text(f'''tap 12 34
tap 12 nope
swipe 1 2 30 40 250
swipe 1 2 3 4
key 4
key 4; touch {injection_marker}
''')
            script = root / 'read-input.sh'
            script.write_text(helper.replace(
                '/system/bin/input', 'input').replace(
                    '/data/droidpc-input.pipe', str(input_file)))
            log = root / 'input.log'
            subprocess.run(['/bin/sh', str(script)], env=os.environ | {
                'PATH': f'{commands}:{os.environ["PATH"]}',
                'INPUT_LOG': str(log),
            }, check=True, timeout=3)

            self.assertEqual(log.read_text().splitlines(), [
                'tap 12 34', 'swipe 1 2 30 40 250', 'keyevent 4',
            ])
            self.assertFalse(injection_marker.exists())

    def test_window_capture_replaces_frame_with_completed_screenshot(self):
        source = (Path(__file__).resolve().parents[1]
                  / 'probe-android-init.sh').read_text()
        helper = source.split(
            'cat > "$tmp/stream-screen.sh" <<\'SH\'\n', 2
        )[2].split('\nSH\n', 1)[0]

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            frame = root / 'frame.png'
            count = root / 'count'
            commands = root / 'bin'
            commands.mkdir()
            screencap = commands / 'screencap'
            screencap.write_text('''#!/bin/sh
count=$(cat "$FRAME_COUNT" 2>/dev/null || echo 0)
[ "$count" -lt 2 ] || { kill -TERM "$PPID"; exit 1; }
echo $((count + 1)) > "$FRAME_COUNT"
printf 'PNG-%s' "$count" > "$2"
''')
            screencap.chmod(0o755)
            helper = helper.replace('/data/droidpc-live-frame.png',
                                    str(frame))
            helper = helper.replace('/system/bin/screencap', 'screencap')
            script = root / 'capture.sh'
            script.write_text(helper)

            result = subprocess.run(['/bin/sh', str(script)], env=os.environ | {
                'PATH': f'{commands}:{os.environ["PATH"]}',
                'FRAME_COUNT': str(count),
            }, timeout=3)

            self.assertEqual(result.returncode, -15)
            self.assertEqual(frame.read_bytes(), b'PNG-1')
            self.assertEqual(frame.stat().st_mode & 0o777, 0o644)

    def test_raw_capture_stream_has_no_png_or_file_polling(self):
        source = (Path(__file__).resolve().parents[1]
                  / 'probe-android-init.sh').read_text()
        helper = source.split(
            'cat > "$tmp/stream-screen.sh" <<\'SH\'\n', 1
        )[1].split('\nSH\n', 1)[0]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            capture = root / 'screencap'
            capture.write_text('''#!/usr/bin/python3
import os, signal, sys
from pathlib import Path
count = Path(os.environ['CAPTURE_COUNT'])
n = int(count.read_text()) if count.exists() else 0
if n == 2:
    os.kill(os.getppid(), signal.SIGTERM)
    sys.exit(1)
assert len(sys.argv) == 1
count.write_text(str(n + 1))
sys.stdout.buffer.write(bytes([n]) * 20)
''')
            capture.chmod(0o755)
            output = root / 'frames'
            script = root / 'capture.sh'
            script.write_text(helper.replace('/system/bin/screencap', str(capture))
                              .replace('/data/droidpc-screen.pipe', str(output))
                              .replace('/data/droidpc-capture.log', str(root / 'capture.log')))
            result = subprocess.run(['/bin/sh', str(script)], env=os.environ | {
                'CAPTURE_COUNT': str(root / 'count'),
            }, timeout=3)
            self.assertEqual(result.returncode, -15)
            self.assertEqual(output.read_bytes(), bytes(20) + bytes([1]) * 20)

    def test_completed_boot_captures_a_screenshot(self):
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
            script.write_text(helper.replace('/data', str(data))
                              .replace('/system/bin/service', 'service')
                              .replace('/system/bin/screencap', 'screencap'))
            state = root / 'boot-property-read'
            stubs = {
                'getprop': '''#!/bin/sh
if [ "$1" = sys.boot_completed ]; then
  if [ -e "$BOOT_PROPERTY_READ" ]; then echo 1
  else touch "$BOOT_PROPERTY_READ"; echo 0; fi
else echo running; fi
''',
                'pidof': '#!/bin/sh\necho 42\n',
                'ps': '#!/bin/sh\necho system_server 42\n',
                'service': '#!/bin/sh\necho "Service $2: found"\n',
                'screencap': '#!/bin/sh\nprintf PNG > "$2"\n',
            }
            for name, contents in stubs.items():
                path = commands / name
                path.write_text(contents)
                path.chmod(0o755)

            environment = os.environ | {
                'PATH': f'{commands}:{os.environ["PATH"]}',
                'BOOT_PROPERTY_READ': str(state),
            }
            subprocess.run(['/bin/sh', str(script)], env=environment,
                           check=True, timeout=3)

            self.assertIn('SCREEN_CAPTURE_OK',
                          (data / 'droidpc-final-state').read_text())
            self.assertEqual((data / 'droidpc-screenshot.png').read_bytes(),
                             b'PNG')

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
