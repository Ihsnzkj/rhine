#!/usr/bin/env python3
"""Known synthetic desktop in the private compositor; no session data."""
import gi
import sys

gi.require_version('Gtk', '4.0')
from gi.repository import Gtk, Gdk, GLib

Gtk.init()
windows = []
monitors = Gdk.Display.get_default().get_monitors()
for i in range(monitors.get_n_items()):
    window = Gtk.Window(title='Rhine synthetic desktop')
    window.set_decorated(False)
    picture = Gtk.Picture.new_for_filename(sys.argv[min(i+1, len(sys.argv)-1)])
    picture.set_content_fit(Gtk.ContentFit.FILL)
    window.set_child(picture)
    window.fullscreen_on_monitor(monitors.get_item(i))
    window.present()
    windows.append(window)
GLib.MainLoop().run()
