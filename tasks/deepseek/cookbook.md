# Commands that are already approved

A refused command costs a turn and teaches you nothing, and the refusals repeat across workers.
These are the shapes that work. A permission rule matches the **whole command**, so a pipe, a
redirect, a `cd` or a `&&` makes a command that would otherwise be allowed fail.

## Research tasks (the shared baseline)

| Works | Refused, and what to do instead |
|---|---|
| `cargo check -p converter` | `cargo build --release` - not in the baseline; a check or a test is enough to answer a research question |
| `cargo test -p converter esm::` | two commands joined with `&&` - run them one at a time |
| `cargo tree -p engine -i bevy_render` | `cd crates/engine && cargo ...` - use `-p <crate>` from the root |
| `python tools/check_citations.py docs/research/x.md` | `python -c "..."` - put it in a file under `tools/research/` and run that |
| `git add docs/research/x.md` then `git commit -m "[deepseek] ..." -- docs/research/x.md` | `git commit -am ...` - it would sweep up another worker's files |
| `git status`, `git diff docs/`, `git log --oneline -20` | `cargo test > out.txt` - read the output from the tool result instead |

Builds share the lead's `target/`. "Blocking waiting for file lock on build directory" means another
build is running: wait for it, do not retry in a loop and never delete `target/`.

## Implementation tasks (an isolated checkout)

| Works | Refused |
|---|---|
| `cargo test -p converter` | anything `git` - the lead integrates, you never commit |
| `cargo clippy -p converter --all-targets -- -D warnings` | editing a file your brief does not list |
| `cargo fmt -p converter` (formats only your crate; check that it touched only your files) | `cargo add`, editing `Cargo.toml` - new dependencies are the lead's decision |
| `python -m unittest tests.test_your_module` | installing packages or toolchains |

## Reading

Prefer the tools over the shell: `Read` a file (including a `.png` - you can see images), `Grep` for
a pattern, `Glob` for names. They are not permission-gated inside the project and the read-only
folders, and they do not spend a turn on quoting.

## When something is genuinely refused

Say so once in your result, with the exact command and the refusal text, and carry on with what you
can do. Do not rewrite the command to sneak past the rule: a script that opens a protected file is
the same act as editing it, and it is the one thing that gets a worker's work thrown away.
