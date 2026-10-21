"""Probe two Windows behaviours this investigation depends on, in a scratch dir.

Usage: python tools/research/phase1_size/link_cap_probe.py

1. How many hard links can one file carry on this volume, and what error does the
   failing call give? (Creates links in a scratch tree under local/.)
2. Does CopyFileW keep the source's creation time? (It keeps the last-write time.)
Writes only under local/research/phase1_size/scratch/.
"""
import ctypes
import os
import shutil
import time
from ctypes import wintypes

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
kernel32.CreateHardLinkW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR, ctypes.c_void_p]
kernel32.CreateHardLinkW.restype = wintypes.BOOL
kernel32.CopyFileW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.BOOL]
kernel32.CopyFileW.restype = wintypes.BOOL
kernel32.SetFileTime.argtypes = [wintypes.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
kernel32.SetFileTime.restype = wintypes.BOOL

SCRATCH = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                       "..", "..", "..", "local", "research", "phase1_size", "scratch")
SCRATCH = os.path.abspath(SCRATCH)

# The failing files live about this deep: D:\...\SkyrimConverted\vfs\textures\terrain\<world>\<name>.dds
DEEP = "vfs/textures/terrain/dlc2apocryphaworld"
NAME = "dlc2apocryphaworld.4.-18.-34.dds"


def link_cap():
    root = os.path.join(SCRATCH, "cap", DEEP.replace("/", os.sep))
    shutil.rmtree(os.path.join(SCRATCH, "cap"), ignore_errors=True)
    os.makedirs(root)
    source = os.path.join(root, NAME)
    with open(source, "wb") as fh:
        fh.write(b"DDS " + b"\0" * 64)
    made = 0
    first_error = None
    for index in range(1500):
        target = os.path.join(root, "link-%04d.dds" % index)
        if kernel32.CreateHardLinkW(target, source, None):
            made += 1
        else:
            error = ctypes.get_last_error()
            if first_error is None:
                first_error = (index, error, error == 1142)
            os.remove(target) if os.path.exists(target) else None
            if index > made + 5:
                break
    print("link cap: %d links created, first failure at index %s (error=%d, ERROR_TOO_MANY_LINKS=%s)"
          % (made, first_error[0], first_error[1], first_error[2]))
    print("  nlink of source now: %d" % os.stat(source).st_nlink)
    print("  path length: %d chars" % len(source))


def copyfile_times():
    root = os.path.join(SCRATCH, "times")
    shutil.rmtree(root, ignore_errors=True)
    os.makedirs(root)
    source = os.path.join(root, "source.dds")
    with open(source, "wb") as fh:
        fh.write(b"DDS ")
    old = time.time() - 4 * 24 * 3600
    handle = kernel32.CreateFileW(source, 0x100, 0, None, 3, 0x80, None)
    hundred_ns = int(old * 10_000_000) + 116444736000000000
    value = ctypes.c_longlong(hundred_ns)
    kernel32.SetFileTime(handle, ctypes.byref(value), None, ctypes.byref(value))
    kernel32.CloseHandle(handle)
    before = os.stat(source)
    target = os.path.join(root, "copy.dds")
    kernel32.CopyFileW(target, source, True)
    after = os.stat(target)
    print("CopyFileW: source ctime=%s / mtime=%s" % (time.ctime(before.st_ctime), time.ctime(before.st_mtime)))
    print("           copy   ctime=%s / mtime=%s" % (time.ctime(after.st_ctime), time.ctime(after.st_mtime)))
    print("  creation time kept: %s; last-write kept: %s"
          % (abs(after.st_ctime - before.st_ctime) < 2, abs(after.st_mtime - before.st_mtime) < 2))


if __name__ == "__main__":
    link_cap()
    copyfile_times()
