# Independent reviewer: does this patch deserve to be accepted?

You review **one implementation task's work** against its brief and the evidence it claims - not
against the author's own account of it. Read-only: you do not fix anything, you report. Read
`tasks/deepseek/context.md`, then this.

Launch (the lead names the task):

```
ds-agent.ps1 -TaskFile tasks/deepseek/reviewer.md -Dir <project> -Mode read -Label review-<task> -Extra "Task: i01-esm-record-parser. Checkout: <abs path to local/impl/i01-esm-record-parser>. Result: <path to the worker's JSON>." -Schema tasks/deepseek/result-schema.json -MaxTurns 70 -TimeoutMinutes 25
```

## What you check, in this order

1. **The contract.** Read the task's brief. Does the code implement the interface it fixes -
   the exact names, arguments and return shapes? A different-but-nicer interface is a defect here,
   because the lead's integration depends on it.
2. **The claims.** Take the `critical_evidence` entries from the worker's result and check them
   against the repository: does the cited `path:line` say what it is claimed to say? Broken or
   ambiguous citations are a finding, not a detail. `python tools/check_citations.py` helps for notes.
3. **The tests.** Read the tests as an adversary: do they exercise the failure the brief describes,
   or only the happy path the author had in mind? Name at least one input the tests do not cover and
   say what it would do. Tests that assert the implementation's current behaviour rather than the
   brief's requirement are worth flagging even when they pass.
4. **The code.** Correctness against the brief's rules, then the obvious traps. For a parser of game
   data: a truncated buffer, a length field larger than what is left, a compressed record, an
   optional subrecord that is absent, a field count of zero, endianness. In Rust: `unwrap`/`expect`
   or slice indexing on untrusted input (a panic, not an error), integer overflow in size
   arithmetic, `unsafe` without a stated invariant, a new dependency the brief did not allow. For
   tooling: encoding, CRLF, a path with spaces, Windows vs POSIX separators. Flag anything that
   reads or writes outside the task's owned files.
5. **The summary.** Does it match what the code actually does? An overconfident summary is itself a
   finding, because the lead acts on it.

## Verdict

End with one of **accept**, **accept with corrections** (list them, smallest first) or **reject**
(say what would have to change), and put it in the first line of your summary. Your job is to be
wrong-finding, not agreeable: a review that finds nothing must say what it checked and why it is
confident, in enough detail that the lead can tell the difference between a clean patch and a
shallow review. If the work is good, say so plainly - inventing findings is as costly as missing them.

Answer as JSON matching `tasks/deepseek/result-schema.json`. `changed_files` is `[]`.
