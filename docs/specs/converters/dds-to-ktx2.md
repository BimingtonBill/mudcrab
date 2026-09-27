# DDS to KTX2 Texture Conversion

OpenSkyrim converts extracted Skyrim DDS assets ahead of time to Basis Universal UASTC in KTX2
containers whose levels are Zstandard-supercompressed (`supercompressionScheme = 2`). The runtime
decompresses the levels and can transcode that payload to the best GPU format available through
Bevy. Supercompression is lossless over the level data: the decoded UASTC blocks are the bytes the
uncompressed container held, so a texture's texels do not depend on whether its levels were
compressed.

## Semantic encoding contract

Color space is derived from the consuming slot, never from a filename suffix or file extension.

| Slot semantics | Encoding |
| --- | --- |
| base color, emissive, specular/gloss, detail, environment cube | sRGB color |
| tangent-space normal, water flow normal | linear normal |
| metallic/roughness, occlusion, height, masks, inner layer, greyscale | linear data |

The converter collects semantics from every published GLB plus `texture_sets` and water rows in the
world database. A texture reached through incompatible classes is rejected because one KTX2 URI
cannot safely represent both transfer functions. Unclassified assets remain linear/data rather than
being guessed from their names.

## Supported DDS topology and formats

- BC1, BC2, BC3, BC4, BC5, BC6H and BC7 2D textures are covered by conversion fixtures.
- Six-face cubemaps and reachable 3D volume textures preserve their topology.
- Ordinary texture arrays are rejected explicitly.
- All authored DDS mip levels are decoded and encoded independently; the converter does not replace
  them with a generated chain. A representation that would lose source levels is a hard failure.
- RGBA alpha is retained for opaque, cutout and blended material consumers.

## Publication and validation

Each mip/face/slice is decoded to RGBA and encoded as UASTC. UASTC is used for every semantic class
because the Bevy runtime path supports its KTX2 supercompression contract consistently. Before an
output is atomically renamed into place, the converter verifies:

- KTX2 signature, transfer function and image-level table;
- width, height, depth, mip count, face count and layer count against the DDS;
- encoded byte length and deterministic SHA-256;
- expanded RGBA byte size, color model and supercompression mode.

The encoder returns raw UASTC levels, so the container is then rewritten level by level: every
level becomes one Zstandard frame, its index entry records the compressed length plus the
uncompressed length, and the header declares scheme 2. Nothing else in the container changes — the
DFD, key/value data, dimensions, transfer function and the sRGB alias handling are copied through —
and Zstandard carries no supercompression global data.

A published texture is reused only when its levels already carry that supercompression. A container
of raw levels left by an earlier conversion (or by a resumed staging directory) is recompressed
from its own levels instead of being copied forward or re-encoded from the DDS; both alternatives
would produce the same texels, one for the cost of a copy and the other for minutes per thousand
textures. Its manifest entry is rewritten with the hash of the bytes actually published, so a
recompression invalidates nothing else and a later run reuses the recompressed file directly.

Temporary files are removed after a failed validation/publication. Batch conversion is staged and
only becomes a complete manifest when every asset and required GLB texture URI validates. The
standalone texture-closure report uses format version 3, records the converter schema and the same
metadata per texture, and never reports success for missing required assets or conversion failures.

## Round-trip acceptance

Automated fixtures transcode the resulting UASTC through the desktop BC7 path and decode it again.
They verify that cutout alpha retains transparent and opaque regions, asymmetric tangent-space
normal vectors keep X/Y orientation and Z intensity, and distinct authored mip colors/alpha remain
in their original levels. Installed cubemap and volume fixtures can also be exercised through the
`OPENSKYRIM_DDS_FIXTURE` and `OPENSKYRIM_VOLUME_DDS_FIXTURE` test environment variables.
