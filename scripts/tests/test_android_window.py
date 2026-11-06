"""Check the Android window's coordinate and input bridge contract."""
import importlib.util
import os
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


SOURCE = Path(__file__).resolve().parents[1] / 'android-window.py'
try:
    spec = importlib.util.spec_from_file_location('android_window', SOURCE)
    window_module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(window_module)
except (ImportError, ValueError):
    window_module = None


@unittest.skipIf(window_module is None, 'Python GTK 3 bindings are unavailable')
class AndroidWindowTests(unittest.TestCase):
    def test_pointer_and_keyboard_handlers_use_focusable_event_box(self):
        source = SOURCE.read_text()

        self.assertIn('self.event_box = Gtk.EventBox()', source)
        self.assertIn("self.event_box.connect('button-press-event'", source)
        self.assertIn("self.event_box.connect('key-press-event'", source)
        self.assertIn('self.event_box.grab_focus()', source)

    def test_refresh_reloads_replaced_screenshot_path(self):
        with tempfile.TemporaryDirectory() as directory:
            frame_path = Path(directory) / 'frame.png'
            frame_path.write_bytes(b'first')
            window = window_module.AndroidWindow.__new__(
                window_module.AndroidWindow
            )
            window.frame_path = str(frame_path)
            window.frame_mtime_ns = None
            window.image = SimpleNamespace(get_allocation=lambda: None)
            first = SimpleNamespace(get_width=lambda: 10, get_height=lambda: 20)
            second = SimpleNamespace(get_width=lambda: 30, get_height=lambda: 40)
            window.resize_frame = lambda *_args: None
            with patch.object(window_module.GdkPixbuf.Pixbuf,
                              'new_from_file', side_effect=[first, second]) as load:
                window.refresh_frame()
                frame_path.write_bytes(b'second')
                os.utime(frame_path, ns=(frame_path.stat().st_atime_ns,
                                         window.frame_mtime_ns + 1_000_000))
                window.refresh_frame()

            self.assertIs(window.frame, second)
            self.assertEqual((window.frame_width, window.frame_height),
                             (30, 40))
            self.assertEqual(load.call_count, 2)

    def test_pointer_coordinates_match_letterboxed_android_frame(self):
        window = window_module.AndroidWindow.__new__(
            window_module.AndroidWindow
        )
        window.frame = object()
        window.frame_width = 1000
        window.frame_height = 2000
        window.image = SimpleNamespace(
            get_allocated_width=lambda: 100,
            get_allocated_height=lambda: 100,
        )

        self.assertIsNone(window.to_android(20, 50))
        self.assertEqual(window.to_android(50, 50), (500, 1000))
        self.assertEqual(window.to_android(74, 50), (980, 1000))

    def test_frame_is_scaled_to_fit_the_window_allocation(self):
        window = window_module.AndroidWindow.__new__(
            window_module.AndroidWindow
        )
        window.frame = window_module.GdkPixbuf.Pixbuf.new(
            window_module.GdkPixbuf.Colorspace.RGB, False, 8, 1000, 2000
        )
        window.frame_width = 1000
        window.frame_height = 2000
        displayed = []
        window.image = SimpleNamespace(
            set_from_pixbuf=displayed.append,
        )

        window.resize_frame(None, SimpleNamespace(width=100, height=100))

        self.assertEqual((displayed[0].get_width(), displayed[0].get_height()),
                         (50, 100))

    def test_tap_and_navigation_key_use_restricted_commands(self):
        window = window_module.AndroidWindow.__new__(
            window_module.AndroidWindow
        )
        window.frame = object()
        window.frame_width = 1000
        window.frame_height = 2000
        window.image = SimpleNamespace(
            get_allocated_width=lambda: 100,
            get_allocated_height=lambda: 200,
        )
        commands = []
        window.send = commands.append
        window.pointer_time = 1.0
        window.pointer_press(None, SimpleNamespace(button=1, x=50, y=100))
        window.pointer_release(None, SimpleNamespace(button=1, x=50, y=100))
        window.key_press(None, SimpleNamespace(
            keyval=window_module.Gdk.KEY_Escape
        ))

        self.assertEqual(commands, ['tap 500 1000', 'key 4'])

    def test_send_writes_one_line_to_input_fifo(self):
        with tempfile.TemporaryDirectory() as directory:
            fifo = Path(directory) / 'input.pipe'
            os.mkfifo(fifo, 0o600)
            reader = os.open(fifo, os.O_RDONLY | os.O_NONBLOCK)
            try:
                window = window_module.AndroidWindow.__new__(
                    window_module.AndroidWindow
                )
                window.input_fifo = str(fifo)
                window.input_fd = None
                window.send('tap 12 34')
                self.assertEqual(os.read(reader, 64), b'tap 12 34\n')
                os.close(window.input_fd)
            finally:
                os.close(reader)


if __name__ == '__main__':
    unittest.main()
