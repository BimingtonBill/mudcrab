"""Read how the Mudcrab team tracks work on GitHub, the same way every time.

The team's model (Chef on #100, 2026-09-30/10-01): an assigned issue is a claim,
a draft PR is work in progress, a comment like "I'm taking this" is a claim by
someone who can't self-assign, and the org Project views group it all.

Prints, in order:
  1. Per person: assigned open issues, and their open PRs (draft/ready) with the
     issues each PR closes.
  2. Unclaimed open issues, with any claim-like comment from the last 14 days.
  3. Open PRs with no linked issue (work nobody can find from the issue list).
  4. Project fields (Status etc.) when the token has read:project; otherwise a hint.

Usage: python tools/team_board.py [--repo Mudcrab-Team/mudcrab]
Read-only: uses gh api GETs and GraphQL queries only.
"""

import argparse
import contextlib
import json
import re
import subprocess
import sys
from collections import defaultdict
from datetime import datetime, timedelta, timezone

CLAIM_RE = re.compile(
    r"\b(i'?m taking|i'?ll take|taking this|claim(ing)?|working on (this|it)|assign me)\b",
    re.IGNORECASE,
)

PAGE = """
query($owner: String!, $name: String!, $after: String) {
  repository(owner: $owner, name: $name) {
    KIND(states: OPEN, first: 100, after: $after) {
      pageInfo { hasNextPage endCursor }
      nodes { BODY }
    }
  }
}
"""

ISSUE_BODY = """
        number title
        assignees(first: 5) { nodes { login } }
        labels(first: 10) { nodes { name } }
        comments(last: 10) { nodes { author { login } createdAt body } }
"""

PR_BODY = """
        number title isDraft
        author { login }
        assignees(first: 5) { nodes { login } }
        closingIssuesReferences(first: 10) { nodes { number } }
"""

PROJECT_BODY = """
  number
  projectItems(first: 3) { nodes {
    project { title }
    fieldValues(first: 20) { nodes {
      ... on ProjectV2ItemFieldSingleSelectValue {
        name field { ... on ProjectV2SingleSelectField { name } } }
      ... on ProjectV2ItemFieldIterationValue {
        title field { ... on ProjectV2IterationField { name } } }
    } }
  } }
"""

# Creation flag: no console window when a hook or scheduled task starts this tool without a console.
NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)
GH_TIMEOUT = 120


def gh_graphql(query, owner, name, after=None):
    cmd = ["gh", "api", "graphql", "-f", f"query={query}", "-f", f"owner={owner}", "-f", f"name={name}"]
    if after:
        cmd += ["-f", f"after={after}"]
    out = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", timeout=GH_TIMEOUT, creationflags=NO_WINDOW)
    return out.returncode, out.stdout, out.stderr


def fetch_all(kind, body, owner, name):
    """Every open node of `kind` (issues or pullRequests), following the 100-per-page cursor.

    Returns (nodes, error); `error` is empty on success and `nodes` then holds everything.
    """
    nodes, after = [], None
    while True:
        code, out, err = gh_graphql(PAGE.replace("KIND", kind).replace("BODY", body), owner, name, after)
        data = None
        with contextlib.suppress(ValueError):
            data = json.loads(out or "null")
        if code != 0 or not data or not data.get("data"):
            return nodes, (err.strip() or out.strip() or f"gh exit {code}")[:300]
        page = data["data"]["repository"][kind]
        nodes += page["nodes"]
        if not page["pageInfo"]["hasNextPage"]:
            return nodes, ""
        after = page["pageInfo"]["endCursor"]


def login_of(node):
    """Login of a GraphQL author; the field is null for a deleted account."""
    return (node.get("author") or {}).get("login") or "ghost"


def logins(node):
    return [a["login"] for a in node["assignees"]["nodes"]]


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default="Mudcrab-Team/mudcrab")
    args = ap.parse_args()
    owner, name = args.repo.split("/")

    issues, issue_err = fetch_all("issues", ISSUE_BODY, owner, name)
    prs, pr_err = fetch_all("pullRequests", PR_BODY, owner, name)
    if issue_err or pr_err:
        raise SystemExit(f"GitHub query failed: {issue_err or pr_err}")

    people = defaultdict(lambda: {"issues": [], "prs": []})
    for i in issues:
        for who in logins(i):
            people[who]["issues"].append(i)
    for p in prs:
        people[login_of(p)]["prs"].append(p)

    print("== 1. Per person ==")
    for who in sorted(people):
        print(f"\n@{who}")
        for i in people[who]["issues"]:
            print(f"  issue #{i['number']}: {i['title'][:70]}")
        for p in people[who]["prs"]:
            state = "draft" if p["isDraft"] else "ready"
            closes = [n["number"] for n in p["closingIssuesReferences"]["nodes"]]
            link = f" closes {', '.join(f'#{n}' for n in closes)}" if closes else ""
            print(f"  PR    #{p['number']} [{state}]{link}: {p['title'][:60]}")

    print("\n== 2. Unclaimed open issues ==")
    since = datetime.now(timezone.utc) - timedelta(days=14)
    for i in issues:
        if logins(i):
            continue
        labels = ",".join(l["name"] for l in i["labels"]["nodes"])
        print(f"  #{i['number']} {{{labels}}}: {i['title'][:70]}")
        for c in i["comments"]["nodes"]:
            when = datetime.fromisoformat(c["createdAt"].replace("Z", "+00:00"))
            if when >= since and CLAIM_RE.search(c["body"] or ""):
                author = login_of(c)
                print(f"      claim? @{author} {c['createdAt'][:10]}: {c['body'][:90]!r}")

    print("\n== 3. Open PRs with no linked issue ==")
    for p in prs:
        if not p["closingIssuesReferences"]["nodes"]:
            state = "draft" if p["isDraft"] else "ready"
            print(f"  #{p['number']} [{state}] @{login_of(p)}: {p['title'][:60]}")

    print("\n== 4. Project fields ==")
    projects = {}
    for kind in ("issues", "pullRequests"):
        projects[kind], err = fetch_all(kind, PROJECT_BODY, owner, name)
        if err:
            if "INSUFFICIENT_SCOPES" in err or "read:project" in err:
                print("  Not readable: the gh token lacks read:project.")
                print("  Fix once, in your own terminal: gh auth refresh -s read:project")
            else:
                print(f"  Not readable: {err}")
            return
    for kind, nodes in projects.items():
        for n in nodes:
            for item in n["projectItems"]["nodes"]:
                fields = [
                    f"{fv['field']['name']}={fv.get('name') or fv.get('title')}"
                    for fv in item["fieldValues"]["nodes"]
                    if fv and fv.get("field")
                ]
                tag = "PR" if kind == "pullRequests" else "issue"
                print(f"  {tag} #{n['number']} [{item['project']['title']}] {' '.join(fields)}")


if __name__ == "__main__":
    main()
