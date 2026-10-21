# One tree, every divergence an option (approved by the user, 2026-10-16)

**Status: approved by the user, 2026-10-16.** It replaces the section "Three lanes" of `AGENTS.md`
(now "One tree"); `docs/pr/two-tracks.md` is history. The tree is branch `dev` (renamed from `plus` 2026-10-16, tag `lane/plus-final`). The reasoning, the option
inventory, the migration order and the open questions are in `local/plans/one-tree-options-policy.md`
(gitignored, local only).

Origin: the user, 2026-10-16: the base/Plus split may be unnecessary; everything goes in one Mudcrab
settings menu, so the player chooses cluster geometry, distant shadows, baked LOD and so on; anything
that diverges from vanilla parity becomes an option, not a different branch.

---

## Replacement text for the AGENTS.md section "Three lanes"

## One tree, options for every divergence (the user, 2026-10-16)
Our tree is branch `dev` (renamed from `plus` 2026-10-16; Mudcrab Plus stays a name for the options; folders and tools may still say
`portal`).
It is `main` plus our work. Everything in it that renders, emits or behaves differently from vanilla
Skyrim is an **option** in the one settings menu, and every option **defaults to vanilla**.

| Kind of change | Where it is built | Default | Can go upstream? |
|---|---|---|---|
| Vanilla parity fix (closer to Skyrim) | `phase*/<topic>` cut from `main` | on (it is vanilla) | yes, as a PR, per-item yes |
| Divergence (portal doors, cluster LOD, SSR water, AO, Water/Flora/Seasons Plus, modern lighting) | `phase*/<topic>` from `main` if it can stand alone, else `opt/<topic>` from `dev` | off | yes once it stands alone and the user says so; never assumed |
| Process, tools, docs, `local/` | any branch | n/a | docs only, per-item yes |

Rules:
- **All options off is vanilla.** With every option at its default, `dev` renders pixel-identical to
  its parity reference: `main` plus the `phase*/*` branches merged into `dev` that are waiting
  upstream. Converter output with every conversion flag off is byte-identical to that reference's.
  The thread that merges into `dev` runs the parity check (`tools/parity_check.py`, shots with all
  options off against the reference build, plus nextest) before the merge, as a TEST run.
- **An option costs nothing when off**: its plugin, systems, views and passes are not added, its
  payload not loaded. Not merely hidden.
- **One menu.** Options register in upstream's settings inventory (PR 229, `settings.rs`) under a
  group, with a vanilla default, a "differs from vanilla" mark, and whether a change needs a restart
  or a reconversion. Presets match Skyrim's own: **Vanilla Low, Medium, High, Ultra** (the shipped
  INI values); **Vanilla High is the engine's default**. The demo and the 160 fps routes pass the
  **Showcase** preset (today's demo look). Hardware-dependent options (clusters for distant
  objects) check the GPU and are greyed with the reason where unsupported. Long-term look: the menu
  styled like Skyrim's own, as if it came with the game.
- **Data:** one converted set. Core schema = `main`'s; anything else (option payloads, parity data
  still waiting upstream) is a named, versioned extension in the manifest. No private numbering.
- **Vanilla needs evidence.** A default is "vanilla" only with a cited record, INI default or Skyrim
  reference capture; an entry without one is marked "vanilla value unknown", never guessed.
- **Converter-side options are conversion flags** (e.g. `--merged-terrain`, cluster payloads, baked
  LOD inventory). Their output is additive: separate tables or files named by the option, each with
  its own version, listed in the manifest. The core schema numbers follow `main`; no private schema
  numbering.
- **Changes still flow one way:** `main` and `phase*/*` merge into `dev`, never the reverse. A
  parity bug found on `dev` is fixed on a `phase*/*` branch first. Merges into `dev` happen in
  `D:/Mudcrab/merge/dev-integration`, never in the main checkout.
- **Build rule: fewer split builds** (the user, 2026-10-16). A new divergence is built straight on
  `dev` behind its option (`opt/<topic>`); its small `phase*/<topic>` branch from `main` is cut only
  when it is ready to send. Parity fixes clearly headed upstream still start from `main` (carving them
  out later drifts). Options are tried and compared in one `dev` build by switching them in the menu,
  not in combination branches or scratch merges. One converted data set serves `main` and `dev`.
  Still separate: one check build per upstream PR branch, and one build folder per thread that
  builds at the same time.
- **Going upstream is still publishing.** An option moves upstream only as its own `phase*/*` branch
  cut from `main`, cleaned to upstream's rules, with the user's yes for that PR. Nothing on `dev` is
  pushed to `origin`. The publishing rule below is unchanged.

Details, the parity check and the migration: `local/plans/one-tree-options-policy.md`.

---

## What this replaces

- `AGENTS.md`, section "Three lanes" (the table Upstream / Plus / Housekeeping and the paragraph
  under it).
- `docs/pr/two-tracks.md` becomes history (a status line at its top points here), as
  `docs/process/agents-full-2026-10-11.md` section "Base and Plus" does.
- The memory facts "Plus never goes upstream" and "keep plus features clearly separated from Mudcrab
  proper" become: "an option goes upstream only with the user's per-item yes; nothing is assumed
  either way".

Names in the text above (`opt/<topic>`, `tools/parity_check.py`) are proposals; the branch rename is done
(`plus` became `dev` on 2026-10-16).
