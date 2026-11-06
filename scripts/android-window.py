#!/usr/bin/python3
"""Display Android frames and forward basic pointer/navigation input."""
import os
import sys
import time

import gi

gi.require_version('Gtk', '3.0')
gi.require_version('Gdk', '3.0')
gi.require_version('Gst', '1.0')
from gi.repository import Gdk, GLib, Gst, Gtk


KEYCODES = {
    Gdk.KEY_Escape: 4,
    Gdk.KEY_Home: 3,
    Gdk.KEY_BackSpace: 67,
    Gdk.KEY_Return: 66,
    Gdk.KEY_KP_Enter: 66,
    Gdk.KEY_Tab: 61,
    Gdk.KEY_space: 62,
    Gdk.KEY_Left: 21,
    Gdk.KEY_Up: 19,
    Gdk.KEY_Right: 22,
    Gdk.KEY_Down: 20,
}


class AndroidWindow:
    def __init__(self, stream_fifo, input_fifo):
        self.input_fifo = input_fifo
        self.input_fd = None
        self.stream_fd = os.open(stream_fifo, os.O_RDWR)
        self.frame_width = 0
        self.frame_height = 0
        self.pointer_start = None
        self.pointer_time = 0.0
        self.pipeline = Gst.parse_launch(
            f'fdsrc fd={self.stream_fd} do-timestamp=true is-live=true '
            '! h264parse config-interval=-1 ! decodebin '
            '! queue max-size-buffers=2 max-size-bytes=0 max-size-time=0 '
            'leaky=downstream ! glupload ! glcolorconvert '
            '! gtkglsink name=video_sink sync=true'
        )
        self.video_sink = self.pipeline.get_by_name('video_sink')
        self.video_sink.set_property('force-aspect-ratio', True)
        self.video_widget = self.video_sink.get_property('widget')
        self.video_sink.get_static_pad('sink').add_probe(
            Gst.PadProbeType.EVENT_DOWNSTREAM, self.read_video_caps
        )
        bus = self.pipeline.get_bus()
        bus.add_signal_watch()
        bus.connect('message', self.pipeline_message)
        self.window = Gtk.Window(title='Droidpc Android')
        self.window.set_default_size(960, 540)
        self.event_box = Gtk.EventBox()
        self.event_box.set_visible_window(False)
        self.event_box.set_can_focus(True)
        self.event_box.add_events(
            Gdk.EventMask.BUTTON_PRESS_MASK
            | Gdk.EventMask.BUTTON_RELEASE_MASK
            | Gdk.EventMask.KEY_PRESS_MASK
        )
        self.video_widget.set_hexpand(True)
        self.video_widget.set_vexpand(True)
        self.event_box.add(self.video_widget)
        self.event_box.connect('button-press-event', self.pointer_press)
        self.event_box.connect('button-release-event', self.pointer_release)
        self.event_box.connect('key-press-event', self.key_press)
        self.window.add(self.event_box)
        self.window.connect('destroy', Gtk.main_quit)
        self.window.show_all()
        self.event_box.grab_focus()
        if self.pipeline.set_state(Gst.State.PLAYING) == Gst.StateChangeReturn.FAILURE:
            raise RuntimeError('GStreamer could not start the Android video pipeline')

    def read_video_caps(self, _pad, info):
        event = info.get_event()
        if event.type == Gst.EventType.CAPS:
            structure = event.parse_caps().get_structure(0)
            self.frame_width = structure.get_value('width')
            self.frame_height = structure.get_value('height')
        return Gst.PadProbeReturn.OK

    def pipeline_message(self, _bus, message):
        if message.type == Gst.MessageType.ERROR:
            error, detail = message.parse_error()
            print(f'Android video pipeline: {error}: {detail}', file=sys.stderr)
            Gtk.main_quit()
        elif message.type == Gst.MessageType.EOS:
            print('Android video stream ended.', file=sys.stderr)
            Gtk.main_quit()

    def stop(self):
        self.pipeline.set_state(Gst.State.NULL)
        os.close(self.stream_fd)

    def send(self, command):
        if self.input_fd is None:
            try:
                self.input_fd = os.open(
                    self.input_fifo, os.O_WRONLY | os.O_NONBLOCK
                )
            except OSError:
                return
        try:
            os.write(self.input_fd, (command + '\n').encode('ascii'))
        except OSError:
            os.close(self.input_fd)
            self.input_fd = None

    def to_android(self, x, y):
        if not self.frame_width or not self.frame_height:
            return None
        width = self.event_box.get_allocated_width()
        height = self.event_box.get_allocated_height()
        frame_width = self.frame_width
        frame_height = self.frame_height
        scale = min(width / frame_width, height / frame_height)
        shown_width = frame_width * scale
        shown_height = frame_height * scale
        x = (x - (width - shown_width) / 2) / scale
        y = (y - (height - shown_height) / 2) / scale
        if x < 0 or y < 0 or x >= frame_width or y >= frame_height:
            return None
        return round(x), round(y)

    def pointer_press(self, _widget, event):
        if event.button == 1:
            self.pointer_start = self.to_android(event.x, event.y)
            self.pointer_time = time.monotonic()
        return True

    def pointer_release(self, _widget, event):
        if event.button != 1 or self.pointer_start is None:
            return False
        start = self.pointer_start
        end = self.to_android(event.x, event.y)
        self.pointer_start = None
        if end is None:
            return True
        x1, y1 = start
        x2, y2 = end
        if abs(x1 - x2) + abs(y1 - y2) < 12:
            self.send(f'tap {x2} {y2}')
        else:
            duration = max(80, min(1000,
                                   int((time.monotonic() - self.pointer_time)
                                       * 1000)))
            self.send(f'swipe {x1} {y1} {x2} {y2} {duration}')
        return True

    def key_press(self, _widget, event):
        keycode = KEYCODES.get(event.keyval)
        if keycode is None:
            name = Gdk.keyval_name(event.keyval) or ''
            if len(name) == 1 and name.isascii() and name.isalpha():
                keycode = ord(name.lower()) - ord('a') + 29
            elif len(name) == 1 and name.isascii() and name.isdigit():
                keycode = int(name) + 7 if name != '0' else 7
        if keycode is not None:
            self.send(f'key {keycode}')
        return True

if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit('usage: android-window.py SCREEN_FIFO INPUT_FIFO')
    initialized, _argv = Gtk.init_check()
    if not initialized:
        raise SystemExit('Android window: cannot connect to the desktop display')
    Gst.init(None)
    window = AndroidWindow(sys.argv[1], sys.argv[2])
    Gtk.main()
    window.stop()
