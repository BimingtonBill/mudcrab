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

QUERY = """
query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    issues(states: OPEN, first: 100) {
      nodes {
        number title
        assignees(first: 5) { nodes { login } }
        labels(first: 10) { nodes { name } }
        comments(last: 10) { nodes { author { login } createdAt body } }
      }
    }
    pullRequests(states: OPEN, first: 100) {
      nodes {
        number title isDraft
        author { login }
        assignees(first: 5) { nodes { login } }
        closingIssuesReferences(first: 10) { nodes { number } }
      }
    }
  }
}
"""

PROJECT_QUERY = """
query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    issues(states: OPEN, first: 100) { nodes { number ...items } }
    pullRequests(states: OPEN, first: 100) { nodes { number ...items } }
  }
}
fragment items on ProjectV2ItemOwner {
  projectItems(first: 3) { nodes {
    project { title }
    fieldValues(first: 20) { nodes {
      ... on ProjectV2ItemFieldSingleSelectValue {
        name field { ... on ProjectV2SingleSelectField { name } } }
      ... on ProjectV2ItemFieldIterationValue {
        title field { ... on ProjectV2IterationField { name } } }
    } }
  } }
}
"""


def gh_graphql(query, owner, name):
    out = subprocess.run(
        ["gh", "api", "graphql", "-f", f"query={query}", "-F", f"owner={owner}", "-F", f"name={name}"],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    return out.returncode, out.stdout, out.stderr


def logins(node):
    return [a["login"] for a in node["assignees"]["nodes"]]


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default="Mudcrab-Team/mudcrab")
    args = ap.parse_args()
    owner, name = args.repo.split("/")

    code, out, err = gh_graphql(QUERY, owner, name)
    if code != 0:
        raise SystemExit(err.strip())
    repo = json.loads(out)["data"]["repository"]
    issues = repo["issues"]["nodes"]
    prs = repo["pullRequests"]["nodes"]

    people = defaultdict(lambda: {"issues": [], "prs": []})
    for i in issues:
        for who in logins(i):
            people[who]["issues"].append(i)
    for p in prs:
        people[p["author"]["login"]]["prs"].append(p)

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
                author = (c["author"] or {}).get("login", "?")
                print(f"      claim? @{author} {c['createdAt'][:10]}: {c['body'][:90]!r}")

    print("\n== 3. Open PRs with no linked issue ==")
    for p in prs:
        if not p["closingIssuesReferences"]["nodes"]:
            state = "draft" if p["isDraft"] else "ready"
            print(f"  #{p['number']} [{state}] @{p['author']['login']}: {p['title'][:60]}")

    print("\n== 4. Project fields ==")
    code, out, err = gh_graphql(PROJECT_QUERY, owner, name)
    if code != 0 or "INSUFFICIENT_SCOPES" in out:
        print("  Not readable: the gh token lacks read:project.")
        print("  Fix once, in your own terminal: gh auth refresh -s read:project")
        return
    data = json.loads(out)["data"]["repository"]
    for kind in ("issues", "pullRequests"):
        for n in data[kind]["nodes"]:
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
