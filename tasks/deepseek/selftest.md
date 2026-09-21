# Worker self-test

Checks that the worker harness itself still works after a change to the launcher, the model,
the project config or the permission rules. Run it, read the verdict, throw the result away.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "C:/Users/Brannock/.claude/skills/deepseek-agents/ds-agent.ps1" -TaskFile "tasks/deepseek/selftest.md" -Dir "C:/Modding/OpenSkyrimProject" -Label selftest -Effort medium -MaxTurns 20 -TimeoutMinutes 6 -Schema "tasks/deepseek/selftest-schema.json"
```

Do exactly these six checks, in order, and nothing else. Do not run git. Do not read or change
anything the checks do not name. Each check is PASS only if it behaved as described.

1. **search-glob** - Glob `crates/**/*.rs`. PASS if it returns 40 or more files.
2. **search-grep** - Grep `rkyv` under `crates/`. PASS if at least one file matches; name one.
3. **reference-read** - Glob `bevy-0.19*/Cargo.toml` in `C:/Users/Brannock/.cargo/registry/src/*/`,
   outside the project, and Read the first match. PASS if you can quote its `version = ` line.
4. **vision** - Read the image `docs/images/phase2-rural-skyrim-assets.png`. PASS if you can see it
   and describe two concrete things in the picture (terrain, sky, an object, a colour).
5. **write-refused** - Try once to create `crates/selftest.tmp` with the text `x`. PASS if it is
   refused. Do not retry, and do not try another route: a refusal is the expected result.
6. **write-allowed** - Create `local/agents/selftest-ok.txt` containing `OK`. PASS if it succeeds.

Report JSON only, matching the schema: one entry per check with its name, PASS or FAIL, and a short
detail (the quoted line, the file name, the refusal message). `verdict` is PASS only if all six pass.
