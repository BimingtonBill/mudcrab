"""Cross-check the NiTriStripsData files against the conversion manifest and GLB output.

For each strips file: is it in the manifest (converted / failed / skipped)? Does its
published GLB contain a bounded POSITION accessor (i.e. is any *visual* geometry
present or lost)? Also searches the manifest keys for pressure-plate style paths,
and reports the record-reachable model counts behind the M1.6 question.
"""
from __future__ import annotations

import json
import struct
import sys
from pathlib import Path


def glb_has_bounded_positions(path: Path) -> bool | None:
    if not path.is_file():
        return None
    data = path.read_bytes()
    if data[:4] != b"glTF" or len(data) < 20:
        return False
    json_len = struct.unpack_from("<I", data, 12)[0]
    doc = json.loads(data[20 : 20 + json_len].decode("utf-8", "replace"))
    for acc in doc.get("accessors", []):
        if acc.get("type") == "VEC3" and "min" in acc and "max" in acc:
            return True
    # empty-scene GLBs have no accessors at all
    return bool(doc.get("accessors"))


def main() -> int:
    root = Path(sys.argv[1])  # converted install root
    scan = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
    manifest = json.loads((root / "conversion-manifest.json").read_text(encoding="utf-8"))
    entries = manifest.get("entries", {})
    failures = manifest.get("failures", {})

    rows = []
    for item in scan["strips_files"]:
        rel = item["path"]  # e.g. architecture/windhelm/whprison02.nif
        # manifest keys are source-relative with backslashes and original case
        candidates = [
            f"meshes\\{rel}".replace("/", "\\"),
            f"meshes\\{rel.replace('/', chr(92))}",
            rel.replace("/", "\\"),
        ]
        key = next((k for k in candidates if k in entries), None)
        entry = entries.get(key) if key else None
        glb_rel = f"meshes/{Path(rel).with_suffix('.glb').as_posix().lower()}"
        glb_path = root / glb_rel
        rows.append(
            {
                "nif": rel,
                "manifest_key_found": key is not None,
                "manifest_key": key,
                "in_failures": any(rel.replace("/", "\\").lower() in k.lower() for k in failures),
                "glb_path": glb_rel,
                "glb_exists": glb_path.is_file(),
                "glb_has_bounded_positions": glb_has_bounded_positions(glb_path),
                "has_BSTriShape": "BSTriShape" in item["all_block_types"],
                "has_bhknistrips": "bhkNiTriStripsShape" in item["all_block_types"],
            }
        )

    # Pressure-plate style paths anywhere in the manifest keys
    plate_hits = sorted(
        k for k in entries if "pressure" in k.lower() or "pressplate" in k.lower() or "plate" in k.lower()
    )
    # Legacy NiTriShape (non-strips) files, to size the M1.6 "legacy geometry" pool
    print(
        json.dumps(
            {
                "strips_crosscheck": rows,
                "strips_total": len(rows),
                "strips_with_manifest_entry": sum(1 for r in rows if r["manifest_key_found"]),
                "strips_with_visual_geometry": sum(
                    1 for r in rows if r["glb_has_bounded_positions"]
                ),
                "strips_with_bhknistrips_shape": sum(1 for r in rows if r["has_bhknistrips"]),
                "strips_with_BSTriShape": sum(1 for r in rows if r["has_BSTriShape"]),
                "pressure_plate_path_hits": plate_hits[:40],
                "pressure_plate_hit_count": len(plate_hits),
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
