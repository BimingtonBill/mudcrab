# Far-terrain cell inventory: persistent containers are not grid placements

The first read-only acceptance of the new F1 adapter against the existing demo
package failed with duplicate projected CELL coordinates in world 60. This was
an adapter inventory error, not evidence requiring a new dataset or relaxed
ambiguity checks. The failed log remains at ignored
`local/vg7/f1-real-package-acceptance.log`.

The diagnostic SQLite read found one duplicated coordinate, `(0,0)`: a CELL
with header flags `0x40400`, no LAND, and an ordinary CELL with flags `0x40000`
and a 1096-byte VHGT. The world has 11,187 projected CELL rows, 11,186 LAND rows
and 11,186 canonical height pages. The diagnosis used a read-only SQLite
transaction; it did not certify the package or change any bytes.

The existing transition research already records that exterior persistent
containers can carry XCLC `(0,0)` while their references occupy many real grids:
`docs/research/worldspace-transition-demo.md`, section 2.2. Therefore neither
XCLC presence nor a preference for the larger FormID establishes a grid role.
The converter exports `cells.flags` from the winning record header
(`crates/converter/src/esm/exporter.rs`, CELL export), separately from the raw
CELL.DATA subrecord retained in `cells.data`.

The authoritative format reference is **xEdit 4.1.5f defs,
Core/wbDefinitionsTES5.pas:6107-6113**: the CELL header defines bit 10,
`0x00000400`, as Persistent. CELL.DATA flags are defined separately later in
the same record. The installed-reference tag is pinned; the older local
world-cell note's uncertainty about this header bit is not used as proof.
[Pinned CELL definitions](https://github.com/TES5Edit/TES5Edit/blob/xedit-4.1.5f/Core/wbDefinitionsTES5.pas#L6106-L6145).
Only format facts were read and cited; no xEdit implementation was copied or
translated into Mudcrab.

The reader recipe now:

- Parses header flags as a valid `u32`, and excludes proven persistent containers
  from the terrain-grid inventory. A persistent CELL carrying LAND is an explicit
  unresolved-role error, including a row without grid coordinates.
- Keeps every ordinary explicit grid cell, including missing or empty LAND as
  Unknown under the existing positive-evidence rules.
- Refuses all remaining duplicate grid claims. Two ordinary cells remain
  ambiguous even if one alone has LAND; there is no ID or LAND-priority tie-break.
- Leaves unlocated rows without an invented coordinate or empty-terrain proof.
  Source completeness, known-empty and closed-world policies remain unchanged.

The reader catalog recipe identifier advances to v2 so a cached stamp cannot
reuse the former inventory interpretation. The published terrain recipe,
database schema, payload formats, geometry and core identity do not change.

Fixtures cover persistent aliases on either side of the real CELL's ID,
ordinary missing LAND, multiple ordinary claimants with zero/one/two LAND rows,
persistent LAND contradictions, invalid header ranges and unrelated header bits.
The actual joint-publication fixtures also include persistent aliases in two
invented worlds. These are targeted reader regressions, not original-ESM
completeness or global visual-parity proof.
