# Optional terrain lighting source

`pbr_functions.wgsl` is derived from Bevy `bevy_pbr` 0.19.0,
`crates/bevy_pbr/src/render/pbr_functions.wgsl`, revision
`c6f634ca9f406d68ba5109d921247b654cb42c10`.
The original is licensed MIT OR Apache-2.0; both license texts are retained
beside this note as `BEVY-LICENSE-MIT` and `BEVY-LICENSE-APACHE`.

The normalized original SHA256 is
`b0c544de95502051f9db70c67ee387d318c101800893915e463225020ffee180`.
Normalization converts CRLF to LF without other changes.
The reviewed derived SHA256 is
`60fdd4c69fd39b7020cf8736137a6dcb7e92ba677511535a60f0c734abb8de60`.

Only three transformations are permitted: change the module import path;
rename the lighting entry and add selected-sun/visibility arguments; multiply
the selected directional light's shadow factor beyond its actual last cascade,
subject to its existing receiving and shadow flags. Other lighting and
post-processing remain the upstream source. Unknown visibility is exactly 1.
This hook is restricted to nontransmissive terrain.

The source is not a global Bevy patch. Runtime registration, actual view/sun
association, packet ownership and hardware layout checks belong to the optional
terrain integration. Merely retaining or composing this source enables no
rendering feature.
