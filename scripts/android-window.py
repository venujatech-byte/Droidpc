#!/usr/bin/python3
"""Display Android frames and forward basic pointer/navigation input."""
import os
import sys
import time

import gi

gi.require_version('Gtk', '3.0')
gi.require_version('Gdk', '3.0')
gi.require_version('GdkPixbuf', '2.0')
from gi.repository import Gdk, GdkPixbuf, GLib, Gtk


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
    def __init__(self, frame_path, input_fifo):
        self.frame_path = frame_path
        self.input_fifo = input_fifo
        self.input_fd = None
        self.frame_mtime_ns = None
        self.frame = None
        self.frame_width = 0
        self.frame_height = 0
        self.pointer_start = None
        self.pointer_time = 0.0
        self.window = Gtk.Window(title='Droidpc Android')
        self.window.set_default_size(960, 540)
        self.image = Gtk.Image()
        self.image.set_can_focus(True)
        self.image.set_hexpand(True)
        self.image.set_vexpand(True)
        self.image.add_events(
            Gdk.EventMask.BUTTON_PRESS_MASK
            | Gdk.EventMask.BUTTON_RELEASE_MASK
            | Gdk.EventMask.KEY_PRESS_MASK
        )
        self.image.connect('button-press-event', self.pointer_press)
        self.image.connect('button-release-event', self.pointer_release)
        self.image.connect('key-press-event', self.key_press)
        self.window.add(self.image)
        self.image.connect('size-allocate', self.resize_frame)
        self.window.connect('destroy', Gtk.main_quit)
        self.window.show_all()
        self.image.grab_focus()
        GLib.timeout_add(60, self.refresh_frame)

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
        if self.frame is None:
            return None
        width = self.image.get_allocated_width()
        height = self.image.get_allocated_height()
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

    def resize_frame(self, _widget, allocation):
        if self.frame is None or allocation.width <= 0 or allocation.height <= 0:
            return
        scale = min(allocation.width / self.frame_width,
                    allocation.height / self.frame_height)
        frame = self.frame.scale_simple(
            max(1, int(self.frame_width * scale)),
            max(1, int(self.frame_height * scale)),
            GdkPixbuf.InterpType.BILINEAR,
        )
        self.image.set_from_pixbuf(frame)

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

    def refresh_frame(self):
        try:
            mtime_ns = os.stat(self.frame_path).st_mtime_ns
        except OSError:
            return True
        if mtime_ns != self.frame_mtime_ns:
            try:
                frame = GdkPixbuf.Pixbuf.new_from_file(self.frame_path)
                self.frame = frame
                self.frame_width = frame.get_width()
                self.frame_height = frame.get_height()
                self.resize_frame(self.image, self.image.get_allocation())
                self.frame_mtime_ns = mtime_ns
            except Exception:
                pass
        return True


if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit('usage: android-window.py FRAME_PNG INPUT_FIFO')
    initialized, _argv = Gtk.init_check()
    if not initialized:
        raise SystemExit('Android window: cannot connect to the desktop display')
    AndroidWindow(sys.argv[1], sys.argv[2])
    Gtk.main()
