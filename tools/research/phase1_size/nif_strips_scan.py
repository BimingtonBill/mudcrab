"""Scan extracted NIF headers under a vfs/meshes root for NiTriStrips blocks.

Header layout matches Skyrim SE (20.2.0.7), as read by the converter's own
`parse_skyrim_header` and confirmed against real bytes:
  line (\\n-terminated) | u32 nif_version | u8 endian | u32 user_version
  | u32 block_count | u32 bethesda_version | 4x sized-string8
  | u16 block_type_count | block_type_count x sized-string32 ...

Reports JSON: files scanned, per-family block-type name counts, and every file
that declares NiTriStrips / NiTriStripsData. Read-only.
"""
from __future__ import annotations

import json
import struct
import sys
from collections import Counter
from pathlib import Path

GEOMETRY_FAMILIES = (
    "BSTriShape",
    "BSDynamicTriShape",
    "BSSubIndexTriShape",
    "BSMeshLODTriShape",
    "BSLODTriShape",
    "NiTriShape",
    "NiTriShapeData",
    "NiTriStrips",
    "NiTriStripsData",
)


class Cur:
    def __init__(self, data: bytes) -> None:
        self.b = data
        self.i = 0

    def take(self, n: int) -> bytes:
        if self.i + n > len(self.b):
            raise ValueError("truncated header")
        out = self.b[self.i : self.i + n]
        self.i += n
        return out

    def u8(self) -> int:
        return self.take(1)[0]

    def u16(self) -> int:
        return struct.unpack("<H", self.take(2))[0]

    def u32(self) -> int:
        return struct.unpack("<I", self.take(4))[0]

    def line(self) -> str:
        nl = self.b.find(b"\n", self.i)
        if nl < 0:
            raise ValueError("unterminated header line")
        s = self.b[self.i : nl].decode("ascii", "replace")
        self.i = nl + 1
        return s

    def ss8(self) -> str:
        n = self.u8()
        return self.take(n).decode("ascii", "replace").rstrip("\0")

    def ss32(self) -> str:
        n = self.u32()
        if n > 1 << 20:
            raise ValueError("implausible sized-string length")
        return self.take(n).decode("ascii", "replace").rstrip("\0")


def parse_block_type_names(data: bytes) -> list[str]:
    c = Cur(data)
    desc = c.line()
    if not desc.startswith("Gamebryo File Format"):
        raise ValueError("not a Gamebryo NIF")
    _nif_version = c.u32()
    endian = c.u8()
    if endian != 1:
        raise ValueError("big-endian NIF")
    _user_version = c.u32()
    block_count = c.u32()
    if block_count > 1_000_000:
        raise ValueError("implausible block count")
    _bethesda_version = c.u32()
    # Skyrim SE 20.2.0.7 carries exactly three sized-string8 fields here
    # (author, process script, export script) before the block-type table;
    # confirmed against real bytes and matching the converter's
    # `parse_skyrim_header` (crates/converter/src/mesh.rs:784-787).
    for _ in range(3):
        c.ss8()
    n_types = c.u16()
    if n_types > 512:
        raise ValueError(f"implausible block-type count {n_types}")
    names = [c.ss32() for _ in range(n_types)]
    return names


def main() -> int:
    root = Path(sys.argv[1])
    out_path = Path(sys.argv[2]) if len(sys.argv) > 2 else None
    files = sorted(root.rglob("*.nif"))
    total = len(files)
    family_counts: Counter[str] = Counter()
    files_with_strips: list[dict] = []
    parse_failures: list[str] = []
    for i, path in enumerate(files):
        try:
            data = path.read_bytes()
        except OSError as exc:
            parse_failures.append(f"{path}: read error {exc}")
            continue
        try:
            names = parse_block_type_names(data)
        except Exception as exc:  # noqa: BLE001 - report and continue
            parse_failures.append(f"{path}: {exc}")
            continue
        for name in names:
            if name in GEOMETRY_FAMILIES:
                family_counts[name] += 1
        strips = [n for n in names if n in ("NiTriStrips", "NiTriStripsData")]
        if strips:
            files_with_strips.append(
                {
                    "path": str(path.relative_to(root)).replace("\\", "/"),
                    "strips_blocks": strips,
                    "all_block_types": names,
                }
            )
        if (i + 1) % 5000 == 0:
            print(f"scanned {i + 1}/{total}", file=sys.stderr)
    report = {
        "root": str(root),
        "files_scanned": total,
        "parse_failures": len(parse_failures),
        "parse_failure_samples": parse_failures[:20],
        "family_counts": dict(sorted(family_counts.items())),
        "files_with_strips": len(files_with_strips),
        "strips_files": files_with_strips[:50],
    }
    text = json.dumps(report, indent=2)
    if out_path:
        out_path.write_text(text, encoding="utf-8")
        print(f"wrote {out_path}")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
