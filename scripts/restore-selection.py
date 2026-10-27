"""Serve a restored multi-format GTK selection from stdin, without disk storage.

Like wl-copy, this owner exits when another application replaces the selection.
GTK's multi-format ownership API is not introspected by Python GI.
"""
import base64
import ctypes as C
import json
import os
import sys

if os.environ.get("GDK_BACKEND") != "x11" or not os.environ.get("ONGLEET_PRIVATE_X11"):
    raise SystemExit("Selection owner is restricted to a private X11 validation display")

gtk = C.CDLL("libgtk-3.so.0")
gdk = C.CDLL("libgdk-3.so.0")
gtk.gtk_init_check.argtypes = [C.c_void_p, C.c_void_p]
gtk.gtk_init_check.restype = C.c_int
if not gtk.gtk_init_check(None, None):
    raise SystemExit("Desktop unavailable")
gdk.gdk_atom_intern.argtypes = [C.c_char_p, C.c_int]
gdk.gdk_atom_intern.restype = C.c_void_p
gtk.gtk_clipboard_get.argtypes = [C.c_void_p]
gtk.gtk_clipboard_get.restype = C.c_void_p
gtk.gtk_selection_data_set.argtypes = [C.c_void_p, C.c_void_p, C.c_int, C.c_void_p, C.c_int]


class Target(C.Structure):
    _fields_ = [("target", C.c_char_p), ("flags", C.c_uint), ("info", C.c_uint)]


get_callback = C.CFUNCTYPE(None, C.c_void_p, C.c_void_p, C.c_uint, C.c_void_p)
clear_callback = C.CFUNCTYPE(None, C.c_void_p, C.c_void_p)
payloads = [(mime.encode(), base64.b64decode(data)) for mime, data in json.load(sys.stdin).items()]
targets = (Target * len(payloads))(*(Target(mime, 0, index) for index, (mime, _) in enumerate(payloads)))


@get_callback
def provide(_clipboard, selection_data, index, _user_data):
    mime, data = payloads[index]
    buffer = C.create_string_buffer(data)
    gtk.gtk_selection_data_set(selection_data, gdk.gdk_atom_intern(mime, False), 8, buffer, len(data))


@clear_callback
def replaced(_clipboard, _user_data):
    gtk.gtk_main_quit()


gtk.gtk_clipboard_set_with_data.argtypes = [C.c_void_p, C.POINTER(Target), C.c_uint, get_callback, clear_callback, C.c_void_p]
gtk.gtk_clipboard_set_with_data.restype = C.c_int
atom = gdk.gdk_atom_intern(b"PRIMARY" if "--primary" in sys.argv else b"CLIPBOARD", False)
if not gtk.gtk_clipboard_set_with_data(gtk.gtk_clipboard_get(atom), targets, len(targets), provide, replaced, None):
    raise SystemExit("Could not restore clipboard ownership")
print("READY", flush=True)
sys.stdout.close()
gtk.gtk_main()
