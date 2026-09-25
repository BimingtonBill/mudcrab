# impl-224 - F12 field notes use the local time

Owned files: crates/engine/src/field_notes.rs,crates/engine/src/pose_capture.rs (only if the shared time helper lives there)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The run folder name (`local/captures/<YYYY-MM-DD_HH-MM-SS>`) and every time in `notes.md` are UTC (`pose_capture::rfc3339`), 9.5 h behind the user's clock. Use the local time for the folder name and the notes' times (keep `saved_at` in NN.json as RFC 3339 UTC, and add a `saved_at_local`). No new dependency: on Windows get the local offset with the standard library or a tiny `windows-sys`/`GetTimeZoneInformation` call only if the crate already depends on it; otherwise say what is needed (dependency changes are the lead's call). Unit tests for the formatting with a fixed offset. Report what you used.
