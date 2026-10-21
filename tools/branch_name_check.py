"""Check that a new branch name can be created before launching a T3 thread on it.

Git stores each branch as a file under refs/heads, so a branch cannot be created below an
existing branch's name (`plus/foo` while `plus` exists) or above one (`phase1` while `phase1/x`
exists). T3's worktree launch then fails in `provision-worktree` with a bare "git worktree add
failed" and no reason (2026-10-14: two threads died this way on `plus/occlusion-proof`).

    python tools/branch_name_check.py <new-branch> [<new-branch> ...]

Exit 0 when every name is free and valid, 1 otherwise, with the reason on stderr.
"""

import subprocess
import sys


def refs():
    out = subprocess.run(
        ["git", "for-each-ref", "--format=%(refname:short)", "refs/heads"],
        check=True, capture_output=True, text=True,
    ).stdout
    return set(out.split())


def problem(name, existing):
    valid = subprocess.run(["git", "check-ref-format", "--branch", name], capture_output=True)
    if valid.returncode != 0:
        return "not a valid branch name"
    if name in existing:
        return "branch already exists"
    parts = name.split("/")
    for depth in range(1, len(parts)):
        parent = "/".join(parts[:depth])
        if parent in existing:
            return f"branch '{parent}' exists, so nothing can be created below '{parent}/'"
    clash = sorted(ref for ref in existing if ref.startswith(name + "/"))
    if clash:
        return f"'{clash[0]}' exists, so '{name}' cannot be a branch"
    return None


def main(names):
    if not names:
        print(__doc__, file=sys.stderr)
        return 2
    existing = refs()
    bad = 0
    for name in names:
        reason = problem(name, existing)
        if reason:
            print(f"{name}: {reason}", file=sys.stderr)
            bad = 1
        else:
            print(f"{name}: ok")
    return bad


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
