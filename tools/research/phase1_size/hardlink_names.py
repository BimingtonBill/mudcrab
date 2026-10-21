"""Every name of a file's inode, via Win32 FindFirstFileNameW (read-only).

Usage: python tools/research/phase1_size/hardlink_names.py <file> [<file> ...]

st_nlink counts links but not their names; this lists them, so a name outside the
tree being examined (a leftover previous output, a second staging tree) is visible.
"""
import ctypes
import sys
from ctypes import wintypes

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
kernel32.FindFirstFileNameW.argtypes = [
    wintypes.LPCWSTR, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), wintypes.LPWSTR]
kernel32.FindFirstFileNameW.restype = wintypes.HANDLE
kernel32.FindNextFileNameW.argtypes = [
    wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD), wintypes.LPWSTR]
kernel32.FindNextFileNameW.restype = wintypes.BOOL
kernel32.FindClose.argtypes = [wintypes.HANDLE]

ERROR_MORE_DATA = 234
ERROR_HANDLE_EOF = 38


def names_of(path):
    length = wintypes.DWORD(4096)
    buffer = ctypes.create_unicode_buffer(4096)
    handle = kernel32.FindFirstFileNameW(path, 0, ctypes.byref(length), buffer)
    if not handle or handle == wintypes.HANDLE(-1).value:
        return ["<error %d>" % ctypes.get_last_error()]
    found = [buffer.value]
    while True:
        length = wintypes.DWORD(4096)
        if kernel32.FindNextFileNameW(handle, ctypes.byref(length), buffer):
            found.append(buffer.value)
        else:
            err = ctypes.get_last_error()
            if err not in (ERROR_HANDLE_EOF, ERROR_MORE_DATA):
                found.append("<error %d>" % err)
            break
    kernel32.FindClose(handle)
    return found


def main():
    for path in sys.argv[1:]:
        names = names_of(path)
        print("%s: %d name(s)" % (path, len(names)))
        for name in names[:20]:
            print("   %s" % name)
        if len(names) > 20:
            print("   ... %d more" % (len(names) - 20))


if __name__ == "__main__":
    main()
