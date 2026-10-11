"""Step-by-step NIF header parse of one file, printing each field and offset."""
from __future__ import annotations

import struct
import sys
from pathlib import Path


def main() -> int:
    data = Path(sys.argv[1]).read_bytes()
    i = 0

    def take(n: int, label: str) -> bytes:
        nonlocal i
        out = data[i : i + n]
        print(f"{i:04x}-{i + n - 1:04x}  {label:22}  {out!r}")
        i += n
        return out

    nl = data.find(b"\n")
    take(nl, "file_desc")
    take(1, "newline")
    take(4, "nif_version " + str(struct.unpack("<I", data[i : i + 4])[0]))
    take(1, "endian")
    take(4, "user_version " + str(struct.unpack("<I", data[i : i + 4])[0]))
    take(4, "block_count " + str(struct.unpack("<I", data[i : i + 4])[0]))
    take(4, "bethesda_version " + str(struct.unpack("<I", data[i : i + 4])[0]))
    for label in ("author", "process_script", "export_script", "max_filepath"):
        n = data[i]
        take(1, f"{label} len={n}")
        take(n, label)
    n_types = struct.unpack("<H", data[i : i + 2])[0]
    take(2, f"block_type_count {n_types}")
    for k in range(n_types):
        n = struct.unpack("<I", data[i : i + 4])[0]
        take(4, f"type[{k}] len={n}")
        take(n, f"type[{k}]")
    print(f"end of block type list at 0x{i:x}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
