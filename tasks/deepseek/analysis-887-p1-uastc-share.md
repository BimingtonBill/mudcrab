# Goal
Find out why so many converted textures are UASTC (KTX2 vkFormat 0, which the engine must transcode to BC7 at load, about 56 ms each) instead of native BC, and whether a converter setting or a small code change fixes it. Phase 1 track. Code reading plus reading small file headers only. This decides today whether the portal demo does one full reconversion.

# Facts measured by the lead (KTX2 header census, read only)
- Portal output `C:/Modding/Mudcrab/SkyrimConverted-portal-lod/textures`: 45,819 KTX2 files.
  - vkFormat 0 (UASTC): 19,605, of which 9,368 are `.opensky-srgb.ktx2` aliases.
  - BC3 (137): 16,262. BC1 (133/134): 6,356. BC4/BC5 (138): 3,424. 135: 172.
  - All zstd-supercompressed (scheme 2).
- Main output `D:/Mudcrab/SkyrimConverted-p2main/textures` (main converter 461517bf, 2026-10-02): 36,423 files.
  - vkFormat 0: 10,241, of which only 4 are `.opensky-srgb` aliases.
  - BC3: 16,271. 134: 3,883. 138: 3,424. 133: 2,432. 135: 172.
- So both have about 10,237 non-alias UASTC textures. The portal also has about 9,364 UASTC sRGB aliases, where main's aliases are native BC.

# Context
- Code: the portal converter in `C:/Modding/Mudcrab/mudcrab/crates/converter/src/` (texture.rs `convert_with_options`, `native_ktx2_format` mapping about :406-449, `convert_packed_to_native`; material.rs, which creates the `.opensky-srgb.ktx2` alias around :665 and :965; texture_gpu/ for the GPU encoder; config.rs and main.rs for `--texture-encoder` and `--portable-textures`). Main's converter: `D:/Mudcrab/worktrees/p1-main-ro/crates/converter/src/` (branch based on current main 2c11897b), for comparison.
- Spec: `docs/specs/converters/dds-to-ktx2.md`.
- Prior notes: `local/research/texture-load-cost.md`, `local/research/phase1-bundle-evidence.md` (said about 247 fallbacks of 33,175). Search `local/research/INDEX.md` for "native", "UASTC", "portable".
- Record/format reference rule: for any Skyrim record or field, cite xEdit 4.1.5f defs. This task is about DDS/KTX2, so DDS format facts come from Microsoft's DDS docs or the code.

# Questions (file:line evidence)
1. Which source DDS formats map to native BC, and which fall back to UASTC? List the fallback conditions: uncompressed RGB(A) formats (before or after #114's RGBA8 support), BC7/BC6H sources, odd sizes, non-multiple-of-4 dimensions, cubemaps, missing mips, `--portable-textures`, the GPU encoder (#119) setting, and anything else.
2. To find which applies, sample about 40 UASTC (vkFormat 0) non-alias files from the portal output plus about 20 portal aliases. For each, find its source DDS in the ingestion cache or vfs and read only the DDS header (128-148 bytes): `C:/Modding/Mudcrab/SkyrimConverted-portal-lod.assets-cache/` or the output's `vfs/` folder, whichever exists. Map the path; read headers only, no full files, and never write anything. Classify each by source format and size.
3. Why the portal's sRGB aliases are UASTC while main's are native: compare the alias path (material.rs and texture.rs) between portal and main, and check whether the portal conversion ran with `--texture-encoder gpu`. The portal-lod conversion settings may be visible in its conversion report or manifest top-level fields; read only top-level keys.
4. Answer plainly: is the fix (a) a setting change only, with no code, (b) a small converter change (say where and roughly how big), or (c) bigger? Say what one full reconversion would then produce: the expected count of UASTC files left.

# Scope
Read only. Never write anything, never run `du`/`dust`, never open a whole texture. No cargo, no engines.

# Report
Under 500 words: answers 1-4 with evidence, a table (source DDS format or condition | count in sample | output format | why), and the one-line verdict for question 4.
