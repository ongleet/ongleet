"""Temporary native diagnostic: compare directory rename APIs with an open child."""
import ctypes as c
from ctypes import wintypes as w
import json
from pathlib import Path
import tempfile

kernel = c.WinDLL("kernel32", use_last_error=True)
kernel.CreateFileW.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, c.c_void_p, w.DWORD, w.DWORD, w.HANDLE]
kernel.CreateFileW.restype = w.HANDLE
kernel.CloseHandle.argtypes = [w.HANDLE]
kernel.MoveFileExW.argtypes = [w.LPCWSTR, w.LPCWSTR, w.DWORD]
kernel.SetFileInformationByHandle.argtypes = [w.HANDLE, c.c_int, c.c_void_p, w.DWORD]
kernel.LockFileEx.argtypes = [w.HANDLE, w.DWORD, w.DWORD, w.DWORD, w.DWORD, c.c_void_p]
invalid = c.c_void_p(-1).value


class Rename(c.Structure):
    _fields_ = [("flags", w.DWORD), ("root", w.HANDLE), ("length", w.DWORD), ("name", c.c_ushort * 1)]


def run(form, child_open, child_lock):
    handles = []
    with tempfile.TemporaryDirectory(prefix="ongleet-rename-probe-") as temporary:
        root = Path(temporary)
        source, target = root / "source", root / "target"
        source.mkdir()
        (source / "metadata.lock").touch()

        def open_handle(path, access, flags=0):
            handle = kernel.CreateFileW(str(path), access, 7, None, 3, flags, None)
            if handle == invalid:
                raise c.WinError(c.get_last_error())
            handles.append(handle)
            return handle

        try:
            if child_open:
                child = open_handle(source / "metadata.lock", 0xC0000000)
                if child_lock:
                    overlapped = c.create_string_buffer(32)
                    if not kernel.LockFileEx(child, 3, 0, 0xFFFFFFFF, 0xFFFFFFFF, overlapped):
                        raise c.WinError(c.get_last_error())
            if form == "move":
                success = kernel.MoveFileExW(str(source), str(target), 0)
            else:
                directory = open_handle(source, 0x10000, 0x02000000)
                text = str(target)
                parent = None
                kind, flags = 22, 2
                if form == "legacy":
                    kind, flags = 3, 0
                elif form == "verbatim":
                    text = "\\\\?\\" + text
                elif form == "nt":
                    text = "\\??\\" + text
                elif form in ("relative", "relative-legacy"):
                    text = target.name
                    parent = open_handle(root, 0xC0000000, 0x02000000)
                    if form == "relative-legacy":
                        kind, flags = 3, 0
                encoded = text.encode("utf-16-le")
                storage = c.create_string_buffer(Rename.name.offset + len(encoded) + 2)
                info = c.cast(storage, c.POINTER(Rename)).contents
                info.flags, info.root, info.length = flags, parent, len(encoded)
                c.memmove(c.addressof(storage) + Rename.name.offset, encoded, len(encoded))
                success = kernel.SetFileInformationByHandle(directory, kind, storage, len(storage))
            error = 0 if success else c.get_last_error()
            print(json.dumps(dict(form=form, child_open=child_open, child_lock=child_lock,
                                  success=bool(success), error=error,
                                  message="" if success else c.FormatError(error))), flush=True)
        finally:
            for handle in reversed(handles):
                kernel.CloseHandle(handle)


for opened, locked in [(False, False), (True, False), (True, True)]:
    for form in ["move", "legacy", "absolute", "verbatim", "nt", "relative", "relative-legacy"]:
        run(form, opened, locked)
