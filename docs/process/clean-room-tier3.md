# Clean-room framework for Tier 3 (disassembly) questions

Status: DRAFT, 2026-10-15. Not legal advice and not reviewed by a lawyer. Until the maintainer records approval for a question (gate 6), Tier 3 stays blocked as it is today. Background and sources: `local/research/legal-clean-room-tier3-2026-10-15.md`. Evidence tiers: `local/research/reference/skyrim-re-digest.md`.

## Quick card (read this first, 30 seconds)

You are about to use Ghidra, a decompile, a disassembly or a debugger trace on a Skyrim binary, or to write from one. Go through the gates in order. Any "no" means stop and take the cheaper route.

| Gate | Question | If no |
|---|---|---|
| 1 Lower tier | Can records (`skyrim_rec.py`), file formats, black-box captures, shipped data (shaders, scripts, configs) or a documented source answer it? | Use that. Tier 3 is the last resort. |
| 2 Interoperability | Is the information needed so Mudcrab (or a mod or SKSE-plugin replacement) can work with the original program, its saves or its data? "Make water look the same" alone is not interoperability. | Do not start. Ask the maintainer. |
| 3 Narrow | Is the question one named behaviour, with a stop condition, and is the answer not readily available elsewhere? | Narrow it or drop it. |
| 4 No DRM | Is the binary unmodified, with no copy-protection removal or bypass of any kind? | Stop. Never touch DRM. |
| 5 Operator | Is the dirty operator the named person in `docs/provenance/OPERATORS.md`, running on their own lawful copy? | Stop. |
| 6 Approval | Does `docs/provenance/<topic>.md` carry the maintainer's written approval line for this exact question? | Stop. Draft the question, wait. |
| 7 Wall | Will the dirty and clean sides be separate sessions, and the clean side a different provider? | Fix the setup first. |

Hard rules for everyone:
- The writer of Mudcrab code never sees a decompile, disassembly, trace, offset or dirty-side transcript. If you saw one by accident, say so in the provenance note and hand the work to another session.
- Nothing from the dirty side enters the repo except a reviewed spec (rules below).
- Never copy code, structure layouts, names, offsets or control flow from the binary or from third-party RE projects (CommonLib, skyrimse-test, etc.). Reading open-source reimplementations (Community Shaders and similar) is Tier 2: read, cite, never copy expressions.
- Never publish a spec, question or provenance note (push, PR, issue) without the maintainer's yes for that item.

## Roles

| Role | Who | Sees | Writes | Never sees |
|---|---|---|---|---|
| Operator (dirty) | One Claude session (Opus) started for this question, with the Ghidra MCP, run on behalf of the named operator | Binary, decompile, game | `D:/Mudcrab/dirty/<topic>/` notes and the draft spec | Mudcrab implementation code for this topic |
| Reviewer | A different session or provider, plus the maintainer | Draft spec only | Pass or fail, redactions | Binary, decompile, dirty notes |
| Clean writer | A fresh session, preferably DeepSeek or another provider | Approved spec, Tier 1 formats, the repo | Rust code and tests | Anything under `D:/Mudcrab/dirty/` |
| Orchestrator | Any session | Approved spec path and the reviewer's verdict | Briefs, provenance note | Dirty notes, decompile output |

Enforcement: Ghidra MCP is loaded only in operator sessions. The clean writer's brief and permissions deny `D:/Mudcrab/dirty/**`. The dirty folder sits outside every worktree and outside git, with an `OWNER.txt`.

## Flow

1. **Question card** (below) written first; maintainer approves it in the provenance file.
2. **Operator pass.** Answers only that question, stops at the stop condition, writes the draft spec. Records its session id.
3. **Lint.** `python tools/spec_lint.py <spec.md>` must pass.
4. **Review.** Reviewer checks the spec against "Spec rules"; maintainer signs off. Failed specs go back to the operator, never forward.
5. **Clean pass.** The clean writer implements from the spec only. Tests derive from the spec's examples and from black-box captures.
6. **Verify.** Compare against reference captures; record the result. Claims that cannot be verified without the binary are marked `dirty-only`.
7. **Provenance note** completed; spec stored in `docs/provenance/<topic>.md` after approval.

## Spec rules

| Allowed | Forbidden |
|---|---|
| Behaviour in plain words | Addresses, offsets, `0x...` values, relative virtual addresses |
| Formulas and constants as facts, with units and ranges | Function, variable, struct or class names taken from the binary or decompile |
| Input and output tables, worked examples | Pseudocode, code blocks, C-like syntax |
| Edge cases and ordering of effects (what happens before what, as observed) | Control-flow structure copied step by step, branch layout, call graphs |
| How to verify the claim without the binary (capture, in-game test, record field) | Struct memory layouts, vtable order, register or calling-convention detail |
| Record and field names from public schemas (xEdit, UESP) | Anything pasted from a decompile window |

Rule of thumb: if two different programmers could implement the behaviour differently from this text, it is a spec; if it dictates one implementation, it is expression and must be cut. Describe what the engine computes, not how the binary arranges it.

## Question card (copy into `docs/provenance/<topic>.md`)

```
Topic:
Question (one behaviour, one sentence):
Why interoperability (what must work with what):
Lower-tier routes tried (record sheets, formats, captures, shipped data) and what each left open:
Stop condition (the answer is complete when):
Not needed (explicitly out of scope):
Operator (named person) and country/lawful copy confirmed:
No DRM touched: yes
Approved by (maintainer) / date:
```

## Provenance record (same file, filled as the work goes)

```
Dirty session id / model / date:
Spec file, sha256:
Lint result:
Reviewer (session or person) / verdict / date:
Clean writer session id / provider / date:
What the clean writer was given (spec path, formats):
What the clean writer never saw (stated):
Accidental exposures (if any) and how handled:
Verification against captures (result):
dirty-only claims left:
```

## Worked example: snow coverage (thread phase2/snow-noise-coverage)

Open questions in `local/research/skyrim/snow-projected-material.md` (noise scale derivation, texProj sign, Normal Dampener, max-angle role, Material UV target, normal source, Community Shaders equivalence).
- Gate 1: black-box route first. With the capture tool, hold lighting fixed, vary the surface angle and the MATO scale/bias, and fit the response. This settles most items without any binary. Also check whether the vanilla shader package can be read as data.
- Gate 2: this is "match the look", not interoperability, so it fails unless the maintainer and a lawyer say engine replacement counts. Expect: no Tier 3 for snow now.
- If it ever passes: one question card per item (for example "what role does Normal Dampener play in the projected weight"), not "how does snow work".

## Where this stands

- Written by a model, unreviewed by a lawyer. Questions for the lawyer are in the research note: whether engine replacement is interoperability, whether shader bytecode analysis is decompilation, whether an AI dirty/clean split is independent creation, the operator's country (Australia: Copyright Act 1968 ss 47D, 47H, 116AN; EU and US differ), and what may be published.
- Existing Tier 3 notes (movement record map, PR #214 render map) predate this framework: treat as cite-only, do not extend, do not base new code on them.
