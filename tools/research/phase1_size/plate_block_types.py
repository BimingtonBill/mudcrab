"""Dump block-type lists for the pressure-plate NIFs (M1.6 legacy-geometry question)."""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from nif_strips_scan import parse_block_type_names  # noqa: E402


def main() -> int:
    root = Path(sys.argv[1])
    out = {}
    for path in sorted(root.rglob("*.nif")):
        rel = str(path.relative_to(root)).replace("\\", "/")
        if "pressure" not in rel.lower():
            continue
        try:
            out[rel] = parse_block_type_names(path.read_bytes())
        except Exception as exc:  # noqa: BLE001
            out[rel] = f"PARSE ERROR: {exc}"
    print(json.dumps(out, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
