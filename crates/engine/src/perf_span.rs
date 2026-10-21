//! Named Tracy spans inside a system.
//!
//! Bevy already spans every system and schedule, so a capture says which system is slow; `perf_span!`
//! splits one big system into its phases (docs/process/profiling-tools.md, "Custom spans"). Build
//! with `--features tracy` and the span shows in the capture; without it the macro expands to
//! nothing, so spans cost nothing in a normal build.
//!
//! Name a span `<area>::<system or stage>::<phase>` with a string literal, for example
//! `lod::visibility::absorb`: one name per phase, never one built at run time, so
//! `tools/perf/tracy_report.py` groups captures from different runs by name. The span lasts until
//! the end of the enclosing block; use a block for a phase that ends before the function does, or
//! the next span nests inside it and `zones.csv` (inclusive time) counts it twice (`zones-self.csv`
//! gives each phase's own time).
//! Keep spans out of per-entity loops: put them around the loop.

#[cfg(feature = "tracy")]
macro_rules! perf_span {
    ($name:literal) => {
        let _perf_span = ::bevy::log::info_span!($name).entered();
    };
}

#[cfg(not(feature = "tracy"))]
macro_rules! perf_span {
    ($name:literal) => {};
}
