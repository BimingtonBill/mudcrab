# R10: legacy sound flags and navigation base-object links

Date: 2026-10-17. Scope: two confirmed in-house reader schema faults.

Format evidence was read before changing the schemas. The allowed xEdit
`dev-4.1.6` branch resolved to commit
`9fb016884bec138ea6c7b872cec831537d464c3e`. These are manually recorded format
facts; no xEdit implementation was copied or translated.

- [SNDR definitions, Core/wbDefinitionsTES5.pas:7503–7541](https://github.com/TES5Edit/TES5Edit/blob/9fb016884bec138ea6c7b872cec831537d464c3e/Core/wbDefinitionsTES5.pas#L7503-L7541):
  pre-35 FNAM is a four-byte unsigned flags word. Bits 0, 1 and 2 are unknown;
  bit 4 is Loop. It is not a localized subtitle. The reader retains the complete
  word, including unknown bits, and its source bytes. This correction does not
  add version-based rejection or change LNAM handling.
- [NAVM definitions, Core/wbDefinitionsTES5.pas:5580–5595](https://github.com/TES5Edit/TES5Edit/blob/9fb016884bec138ea6c7b872cec831537d464c3e/Core/wbDefinitionsTES5.pas#L5580-L5595):
  ONAM is an array of unrestricted FormIDs, without a record-kind whitelist.
  Live REFR targets therefore remain valid. Normal master/load-order remapping,
  live-target checking and malformed-width diagnostics still apply.

The supplied older line ranges differ from this pinned revision; the linked
ranges above are the ones inspected. The existing record index and SNDR sheet
were read using `tools/skyrim_rec.py` from the integration tools. The index warns
that its sheets are stale; there is no NAVM sheet. Neither is treated as the
authority over the pinned primary format definition.

Regression fixtures use authored bytes only. They cover localized SNDR flags
0x11/0x12 with no string bank, unknown-bit retention, typed JSON and runtime
archive publication, remapped live REFR links and preserved native ONAM source
bytes, missing targets, invalid master indices and malformed field widths.
Existing localization tests now use MESG/FULL (a real STRINGS field) instead of
the erroneous SNDR/FNAM interpretation. The shared audio dummy fixture now
encodes a four-byte flags word.

No completeness/warning policy, asset reuse precondition, EXPL interpretation or
other ONAM anomaly is changed. The 26 unusual 128-byte ONAM errors and legacy
40/44-byte EXPL floats remain unresolved; this patch does not establish global
record completeness. No real conversion, GPU run or publication is part of R10.

Validation uses the new owned target
`D:/Mudcrab/build/r10-c7478a9b-20261017` through the shared build gate. Formatting,
whitespace checks and converter all-target/all-feature clippy with `-D warnings`
passed. Cargo's remaining future-compatibility notice is in the external
`proc-macro-error2 v2.0.1` dependency: private `proc_macro` re-export (E0365).
No dependency change is included.

Focused nextest passed: 22 tests, zero failures, 622 unselected tests skipped.
The selection covers all tests in the audio, world/navigation and localization
retention binaries, the three new R10 regressions, and three existing adapter10
tests also matched by the substring filter. Full output was inspected and kept
at `D:/Mudcrab/build/r10-c7478a9b-20261017/focused-tests.log`.

```text
cargo nextest run -p converter --lib --test inhouse_audio_extras --test inhouse_world_extras --test inhouse_localization_retention -E 'test(r10_) | binary(inhouse_audio_extras) | binary(inhouse_world_extras) | binary(inhouse_localization_retention)'
```

This is bounded synthetic verification, not workspace or real-data acceptance.
