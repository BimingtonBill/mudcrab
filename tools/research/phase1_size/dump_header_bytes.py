"""Hex+ASCII dump of the first N bytes of a NIF, to confirm the header layout."""
from __future__ import annotations

import sys
from pathlib import Path


def main() -> int:
    path = Path(sys.argv[1])
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 256
    data = path.read_bytes()[:n]
    for off in range(0, len(data), 16):
        chunk = data[off : off + 16]
        hexpart = " ".join(f"{b:02x}" for b in chunk)
        asc = "".join(chr(b) if 32 <= b < 127 else "." for b in chunk)
        print(f"{off:04x}  {hexpart:<48}  {asc}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
