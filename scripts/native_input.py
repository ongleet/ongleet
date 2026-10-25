"""Temporary compositor input for native GNOME validation; no settings changes."""


class GnomeKeyboard:
    def __enter__(self):
        from gi.repository import Gio, GLib
        self.Gio, self.GLib = Gio, GLib
        self.connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.destination = "org.gnome.Mutter.RemoteDesktop"
        self.interface = self.destination + ".Session"
        self.path = self.connection.call_sync(
            self.destination, "/org/gnome/Mutter/RemoteDesktop", self.destination,
            "CreateSession", None, None, Gio.DBusCallFlags.NONE, 3000, None,
        ).unpack()[0]
        self.symbols = set()
        self.call("Start")
        return self

    def call(self, method, parameters=None):
        return self.connection.call_sync(self.destination, self.path, self.interface,
                                         method, parameters, None,
                                         self.Gio.DBusCallFlags.NONE, 3000, None)

    def keysym(self, symbol, down):
        self.call("NotifyKeyboardKeysym", self.GLib.Variant("(ub)", (symbol, down)))
        if down:
            self.symbols.add(symbol)
        else:
            self.symbols.discard(symbol)

    def tap_sym(self, symbol):
        self.keysym(symbol, True)
        self.keysym(symbol, False)

    def __exit__(self, *_):
        try:
            for symbol in list(self.symbols):
                self.keysym(symbol, False)
        finally:
            self.call("Stop")
