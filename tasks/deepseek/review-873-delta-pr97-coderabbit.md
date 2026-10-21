# Goal
Check a small fix delta on open PR #97 "perf(converter): validate generated artifacts in parallel" (Mudcrab-Team/mudcrab, Rust), made for two CodeRabbit review points. Be strict. Decide whether it fixes both points and breaks nothing.

# Context
- Delta: `local/review/pr-97/delta-cr.diff` (46472362 -> d7d339d6). File after: `local/review/pr-97/head-cr/crates/converter/src/pipeline.rs`. No git is available; read the files. Read code only from the review folder; the checked-out `crates/` is a different branch.
- CodeRabbit point 1 (major): after a failure, parallel validation kept reading later artifacts, where the sequential version returned at once. Fix: an AtomicUsize holds the lowest failing index; an artifact with a higher index is skipped; lower ones are still validated, so the reported error stays the first in list order (the Mutex keeps the lowest-index report).
- CodeRabbit point 2 (minor): the GLB branch read the whole file but checked only its length and the first 4 bytes. Fix: open and read_exact the 12-byte header; UnexpectedEof means an invalid GLB; other IO errors propagate. CodeRabbit said not to add a KTX2 size limit.
- New test: a 6-byte GLB is invalid.

# Scope
Read only. Change nothing.

# Done when
Both points are checked: correct, race-free (Relaxed ordering is enough for a skip hint? the reported error must stay deterministic), tested, nothing that worked before now fails (error messages, IO errors, the existing two-bad-files determinism test).

# Report
- Verdict line: ready / small fixes / fix first.
- Findings as `file:line - issue - why it matters - suggested fix`, at most 4, verified only, each marked blocker / should fix / nit.
- Under 250 words.
