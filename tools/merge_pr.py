#!/usr/bin/env python3
"""Merge one upstream PR by the project's standard steps (SEND-CHECKLIST.md, "Merging a PR").

    python tools/merge_pr.py <number> --summary "one sentence" [--approve] [--dry-run]

Steps, in order; it stops at the first one that fails and says what to do:
  1. The PR is open, not a draft, not yours (nobody merges their own PR) and has no open review threads.
  2. Behind main: update the branch, then wait for CI. A token without the `workflow` scope cannot
     update a branch when main changed a workflow file; the message says how to fix it.
  3. Wait for every check to pass (`tests_pass`).
  4. With --approve: approve the PR, with the AI note (only after the user said yes to that PR).
  5. Squash-merge: title "<PR title> (#N)", body = --summary + one Co-authored-by trailer per distinct
     co-author (minus the PR author and you).
  6. Check on origin/main that the trailers survived.

One PR at a time: after a merge every other open PR is behind main again. Run this once per PR.
--dry-run prints what would happen and changes nothing.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import time

REPO = "Mudcrab-Team/mudcrab"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)  # no console flash when started without a console
GH_TIMEOUT = 300
MERGEABLE = ("CLEAN", "HAS_HOOKS")  # mergeStateStatus values that mean "go"
COMMIT_CAP = 100  # gh pr view --json commits stops here
AI_NOTE = "> Review done with an AI assistant, checked by @{user}."


def gh(*args, check=True):
    try:
        result = subprocess.run(["gh", *args], capture_output=True, text=True, encoding="utf-8", timeout=GH_TIMEOUT,
                                creationflags=NO_WINDOW)
    except subprocess.TimeoutExpired:
        sys.exit(f"gh {' '.join(args[:3])} timed out after {GH_TIMEOUT} s")
    if check and result.returncode:
        sys.exit(f"gh {' '.join(args[:3])} failed: {(result.stderr or result.stdout).strip()}")
    return result


def view(number, fields):
    return json.loads(gh("pr", "view", str(number), "--repo", REPO, "--json", fields).stdout)


def open_threads(number):
    owner, name = REPO.split("/")
    query = (
        "query($n:Int!,$after:String){repository(owner:\"%s\",name:\"%s\"){pullRequest(number:$n)"
        "{reviewThreads(first:100,after:$after){pageInfo{hasNextPage endCursor}nodes{isResolved}}}}}" % (owner, name)
    )
    unresolved, after = 0, None
    while True:
        args = ["api", "graphql", "-f", f"query={query}", "-F", f"n={number}"]
        if after:
            args += ["-f", f"after={after}"]
        threads = json.loads(gh(*args).stdout)["data"]["repository"]["pullRequest"]["reviewThreads"]
        unresolved += sum(1 for node in threads["nodes"] if not node["isResolved"])
        if not threads["pageInfo"]["hasNextPage"]:
            return unresolved
        after = threads["pageInfo"]["endCursor"]


def co_authors(pr, me):
    """Distinct co-authors of the PR's commits, minus the PR author and the person merging."""
    skip = {pr["author"]["login"].lower(), me.lower()}
    # A commit's author may carry no GitHub login (an e-mail not linked to the account): match the
    # PR author's display name too.
    skip_names = {(pr["author"].get("name") or "").strip().lower()} - {""}
    if len(pr["commits"]) >= COMMIT_CAP:
        print(f"warning: gh lists only the first {COMMIT_CAP} commits; co-authors of later commits may be missing")
    found = {}
    for commit in pr["commits"]:
        for author in commit["authors"]:
            if (author.get("login") or "").lower() in skip or author["name"].strip().lower() in skip_names:
                continue
            found.setdefault(author["email"].lower(), f"{author['name']} <{author['email']}>")
        for line in commit.get("messageBody", "").splitlines():
            match = re.match(r"co-authored-by:\s*(.+<(.+)>)\s*$", line, re.I)
            if (
                match
                and match.group(2).lower() not in found
                and match.group(1).split("<")[0].strip().lower() not in skip_names
            ):
                found[match.group(2).lower()] = match.group(1).strip()
    # The same person under two e-mail addresses is one trailer: keep the first per display name.
    seen, trailers = [], []
    for text in found.values():
        key = set(text.split("<")[0].lower().split())
        if not any(same_person(key, other) for other in seen):
            seen.append(key)
            trailers.append(text)
    return trailers


def same_person(first, second):
    """Name word sets that are equal, or where one has two or more words and sits inside the other
    ("Jo Smith" and "Jo Q Smith"). A single word ("al") never matches a longer name ("alice")."""
    small, big = sorted((first, second), key=len)
    return small == big or (len(small) >= 2 and small <= big)


def settled_state(number):
    """The merge state, once GitHub has worked it out (it reports UNKNOWN for a few seconds)."""
    for _ in range(12):
        state = view(number, "mergeStateStatus")["mergeStateStatus"]
        if state != "UNKNOWN":
            return state
        time.sleep(5)
    sys.exit(f"GitHub still reports PR {number} as UNKNOWN after a minute; try again shortly")


def update_branch(number, dry):
    if dry:
        print(f"[dry-run] would update the branch of PR {number} from main")
        return
    update = gh("api", "-X", "PUT", f"repos/{REPO}/pulls/{number}/update-branch", check=False)
    if update.returncode:
        text = update.stderr + update.stdout
        if "workflow" in text and "scope" in text:
            sys.exit(
                "main changed a workflow file and the gh token lacks the `workflow` scope.\n"
                "The user runs once in their own terminal:  gh auth refresh -s workflow\n"
                "(or clicks 'Update branch' on the PR page). Then run this again."
            )
        sys.exit(f"update-branch failed: {text.strip()}")
    time.sleep(30)  # let the new run register before watching it


def wait_for_checks(number, dry):
    if dry:
        print(f"[dry-run] would wait for CI on PR {number}")
        return
    print("waiting for CI (about 7 to 20 minutes)...", flush=True)
    result = gh("pr", "checks", str(number), "--repo", REPO, "--watch", "--fail-fast", check=False)
    if result.returncode:
        sys.exit(f"CI is not green on PR {number}:\n{result.stdout[-1200:]}{result.stderr[-400:]}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    parser.add_argument("number", type=int)
    parser.add_argument("--summary", required=True, help="one sentence for the squash commit body")
    parser.add_argument("--approve", action="store_true", help="approve first (needs the user's yes for this PR)")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    dry, number = args.dry_run, args.number

    me = gh("api", "user", "--jq", ".login").stdout.strip()
    pr = view(number, "title,state,isDraft,author,mergeStateStatus,commits")
    if pr["state"] != "OPEN":
        sys.exit(f"PR {number} is {pr['state']}, nothing to merge")
    if pr["isDraft"]:
        sys.exit(f"PR {number} is a draft; the author marks it ready first")
    if pr["author"]["login"].lower() == me.lower():
        sys.exit(f"PR {number} is yours; nobody merges their own PR (AGENTS.md)")
    threads = open_threads(number)
    if threads:
        sys.exit(f"PR {number} has {threads} open review thread(s); they must be resolved first")
    for attempt in range(3):  # main may move while CI runs: update and wait again
        state = settled_state(number)
        print(f"PR {number} ({pr['title']}) by {pr['author']['login']}: {state}")
        if state == "DIRTY":
            sys.exit(f"PR {number} conflicts with main; the author resolves it (we do not edit their branch)")
        if state == "BEHIND":
            update_branch(number, dry)
        elif attempt:
            break  # CI ran on an up-to-date branch
        wait_for_checks(number, dry)
        if dry or state != "BEHIND":
            break
    else:
        sys.exit(f"PR {number} is still behind main after 3 updates; main keeps moving, try again")

    if args.approve:
        note = AI_NOTE.format(user=me)
        body = f"I read the diff and CI is green on the current branch. Approving.\n\n{note}"
        if dry:
            print(f"[dry-run] would approve PR {number}")
        else:
            gh("pr", "review", str(number), "--repo", REPO, "--approve", "--body", body)
            time.sleep(3)

    if not dry:  # CI can take 20 minutes: look again right before the merge
        now = view(number, "state,isDraft")
        if now["state"] != "OPEN" or now["isDraft"]:
            sys.exit(f"PR {number} changed while waiting (state {now['state']}, draft {now['isDraft']}); not merged")
        threads = open_threads(number)
        if threads:
            sys.exit(f"PR {number} got {threads} open review thread(s) while waiting; resolve them first")
        final = settled_state(number)
        if final not in MERGEABLE:
            sys.exit(f"PR {number} is {final}, not ready to merge (needs a review, a green check or an update?)")
    pr = view(number, "title,author,commits")
    trailers = co_authors(pr, me)
    body = args.summary.strip()
    if trailers:
        body += "\n\n" + "\n".join(f"Co-authored-by: {line}" for line in trailers)
    title = pr["title"] if pr["title"].endswith(f"(#{number})") else f"{pr['title']} (#{number})"
    print(f"squash title: {title}\nsquash body:\n{body}")
    if dry:
        print("[dry-run] nothing merged")
        return
    merged = gh("pr", "merge", str(number), "--repo", REPO, "--squash", "--subject", title, "--body", body, check=False)
    if merged.returncode:
        sys.exit(f"merge refused: {(merged.stderr or merged.stdout).strip()}")
    state = view(number, "state,mergedAt")
    print(f"PR {number}: {state['state']} {state['mergedAt']}")

    fetched = subprocess.run(["git", "fetch", "origin", "main", "-q"], cwd=ROOT, capture_output=True, creationflags=NO_WINDOW)
    logged = subprocess.run(["git", "log", "-1", "--format=%b", "origin/main"], cwd=ROOT, capture_output=True, text=True,
                            creationflags=NO_WINDOW)
    if fetched.returncode or logged.returncode:
        print("could not read origin/main to check the trailers; check them by hand")
        return
    message = logged.stdout
    missing = [line for line in trailers if line not in message]
    print("trailers survived on origin/main" if not missing else f"TRAILERS MISSING on main: {missing}")
    print("Tell the session that built this PR so it can offer its worktree and build folder for cleanup.")


if __name__ == "__main__":
    main()
