"""One Trello-style board of our upstream work: branches, PR drafts and PRs.

Combines the project's earlier tracking pieces so every session reads them the same way:
  - GitHub (as tools/team_board.py does): our open PRs (draft or ready), PRs by others we are part of,
    and our PRs merged in the last 14 days.
  - docs/pr/upstream-queue.md, the "Waiting to send" table (Upstream Dev's send queue).
  - local/pr-drafts/*.md: text drafts whose "# Status" line says it is not sent yet.
  - local/roadmap/*.json: the sessions' item files (format in local/roadmap/README.md); `needs_user`
    items go to "Needs you".
  - local branches phase1/*, phase2/*, phase3/*, portal* with a commit in the last 7 days that are
    not already a PR head or a queue row.

Usage:
  python tools/work_board.py            # summary in the terminal
  python tools/work_board.py --json     # the board as JSON
  python tools/work_board.py --widget   # an HTML fragment for Claude's inline widget (show_widget)
Read-only: git reads, gh GETs, file reads.
"""

import argparse
import glob
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timedelta, timezone

REPO = os.environ.get("UPSTREAM_REPO", "Mudcrab-Team/mudcrab")
ME = os.environ.get("UPSTREAM_ME", "BimingtonBill")
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
COLUMNS = [
    ("needs_you", "Needs you"),
    ("in_progress", "In progress"),
    ("ready", "Ready to send"),
    ("drafts", "Text drafts"),
    ("open", "Open upstream"),
    ("merged", "Merged (14 days)"),
]
BRANCH_RE = re.compile(r"^(phase[123]/|portal|plus-|opt/)")
# Our tree: branch `dev` (renamed from `plus` 2026-10-16, one-tree options policy); `plus` is accepted until the rename.
TREE_BRANCHES = ("dev", "plus")
NOT_SENT_RE = re.compile(r"\b(DRAFT|not sent|prepared|waiting for the user|waits for the user)\b", re.IGNORECASE)
SENT_RE = re.compile(r"^\s*(POSTED|SENT|OPENED|MERGED|RETIRED|DROPPED|SUPERSEDED)", re.IGNORECASE)


NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)  # no console flash when started without a console


def run(args: list[str]) -> str:
    """Local git reads: a failure (for example a missing ref) is an empty answer."""
    out = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", cwd=ROOT, timeout=60, creationflags=NO_WINDOW)
    return out.stdout if out.returncode == 0 else ""


def gh_json(args: list[str]) -> list:
    """A gh listing. A failure stops the board: an empty board would pass for "nothing open"."""
    try:
        out = subprocess.run(["gh", *args], capture_output=True, text=True, encoding="utf-8", cwd=ROOT, timeout=120,
                             creationflags=NO_WINDOW)
    except subprocess.TimeoutExpired:
        raise SystemExit(f"gh {' '.join(args[:2])} timed out after 120 s") from None
    if out.returncode:
        raise SystemExit(f"gh {' '.join(args[:2])} failed: {(out.stderr or out.stdout).strip()[:300]}")
    return json.loads(out.stdout or "[]")


def lane_of(text: str) -> str:
    """The track a card belongs to, from a branch name, file name or title."""
    lowered = text.lower()
    for token, lane in (("phase1", "phase 1"), ("p1-", "phase 1"), ("phase2", "phase 2"), ("p2-", "phase 2"),
                        ("phase3", "phase 3"), ("launcher", "phase 3"), ("portal", "dev"), ("plus", "dev"),
                        ("opt/", "dev")):
        if token in lowered:
            return lane
    return "upstream"


def card(column: str, title: str, *, lane: str, sub: str = "", meta: str = "", url: str = "",
         tag: str = "", ref: str = "") -> dict:
    return {"column": column, "title": title, "lane": lane, "sub": sub, "meta": meta, "url": url,
            "tag": tag, "ref": ref or title}


def github_cards(heads: set[str]) -> list[dict]:
    cards = []
    fields = "number,title,isDraft,headRefName,updatedAt,author,url,reviewDecision,statusCheckRollup"
    ours = gh_json(["pr", "list", "-R", REPO, "--author", ME, "--json", fields, "--limit", "60"])
    others = gh_json(["pr", "list", "-R", REPO, "--search", f"involves:{ME} -author:{ME}",
                      "--json", fields, "--limit", "40"])
    for pr, mine in [(p, True) for p in ours] + [(p, False) for p in others]:
        heads.add(pr["headRefName"])
        checks = [c.get("conclusion") or c.get("state") for c in pr.get("statusCheckRollup") or []]
        failing = sum(1 for c in checks if c in ("FAILURE", "ERROR", "CANCELLED", "TIMED_OUT"))
        ci = "CI failing" if failing else ("CI green" if checks else "no CI")
        who = "ours" if mine else pr["author"]["login"]
        cards.append(card("open", f"PR #{pr['number']} {pr['title']}", lane=lane_of(pr["headRefName"] + pr["title"]),
                          sub=f"{who} · {'draft' if pr['isDraft'] else 'ready'} · {ci}",
                          meta=f"updated {pr['updatedAt'][:10]}", url=pr["url"],
                          tag="draft" if pr["isDraft"] else ("failing" if failing else ""),
                          ref=f"PR #{pr['number']}"))
    since = (datetime.now(timezone.utc) - timedelta(days=14)).strftime("%Y-%m-%d")
    merged = gh_json(["pr", "list", "-R", REPO, "--author", ME, "--state", "merged",
                      "--json", "number,title,mergedAt,headRefName,url", "--limit", "40"])
    for pr in sorted(merged, key=lambda p: p["mergedAt"], reverse=True):
        if pr["mergedAt"][:10] < since:
            continue
        heads.update((pr["headRefName"], f"#{pr['number']}"))
        cards.append(card("merged", f"PR #{pr['number']} {pr['title']}", lane=lane_of(pr["headRefName"] + pr["title"]),
                          meta=f"merged {pr['mergedAt'][:10]}", url=pr["url"], ref=f"PR #{pr['number']}"))
    return cards


def queue_cards(rows_seen: set[str]) -> list[dict]:
    path = os.path.join(ROOT, "docs", "pr", "upstream-queue.md")
    try:
        lines = open(path, encoding="utf-8").read().splitlines()
    except OSError:
        return []
    cards, inside = [], False
    for line in lines:
        if line.startswith("## Waiting to send"):
            inside = True
            continue
        if inside and line.startswith("## "):
            break
        if not inside or not line.startswith("| ") or line.startswith("| Branch") or line.startswith("|---"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 5:
            continue
        name = re.sub(r"[*`]", "", cells[0])
        branches = re.findall(r"(phase[123]/[\w./-]+|portal[\w./-]*)", name)
        rows_seen.update(branches)
        seen_portal = cells[6] if len(cells) > 6 else ""
        cards.append(card("ready", name[:110], lane=lane_of(name), sub=cells[4][:160],
                          meta=f"issue {cells[2]}" if cells[2] and cells[2] != "-" else "",
                          tag="not seen in dev" if seen_portal.startswith("no") else "",
                          ref=branches[0] if branches else name[:60]))
    return cards


def draft_cards(heads: set[str]) -> list[dict]:
    cards = []
    for path in sorted(glob.glob(os.path.join(ROOT, "local", "pr-drafts", "**", "*.md"), recursive=True)):
        name = os.path.relpath(path, os.path.join(ROOT, "local", "pr-drafts")).replace("\\", "/")
        if name.upper() in ("STYLE.MD", "SEND-CHECKLIST.MD") or "/archive/" in name:
            continue
        try:
            head = open(path, encoding="utf-8").read(1500).splitlines()
        except OSError:
            continue
        if not head or not head[0].startswith("# Status") or len(head) < 2:
            continue
        status = head[1]
        if SENT_RE.match(status) or not NOT_SENT_RE.search(status):
            continue
        # Status lines are not always updated on send: a draft whose branch is already a PR head is done.
        if any(b in heads for b in re.findall(r"(phase[123]/[\w./-]+|portal[\w./-]*)", status)):
            continue
        if any(f"#{n}" in heads for n in re.findall(r"#(\d+)", status[:160])):
            continue
        # File times changed in the 2026-10-07 folder move, so the newest date written in the header decides.
        dates = re.findall(r"20\d\d-\d\d-\d\d", " ".join(head[:4]))
        mtime = datetime.strptime(max(dates), "%Y-%m-%d") if dates else datetime.fromtimestamp(os.path.getmtime(path))
        if mtime < datetime.now() - timedelta(days=5):
            continue
        cards.append(card("drafts", name.removesuffix(".md"), lane=lane_of(name), sub=status[:160],
                          meta=f"edited {mtime:%Y-%m-%d}", tag="needs yes" if "yes" in status.lower() else "",
                          ref=f"local/pr-drafts/{name}"))
    return cards


def roadmap_cards() -> list[dict]:
    cards = []
    for path in glob.glob(os.path.join(ROOT, "local", "roadmap", "*.json")):
        try:
            data = json.load(open(path, encoding="utf-8"))
        except (OSError, ValueError):
            continue
        for item in data.get("items", []):
            status = str(item.get("status", "")).replace(" ", "-")
            try:
                fresh = datetime.strptime(str(item.get("updated", ""))[:10], "%Y-%m-%d") > datetime.now() - timedelta(days=14)
            except ValueError:
                fresh = False
            if not fresh:
                continue
            if item.get("needs_user"):
                column = "needs_you"
            elif status == "in-progress":
                column = "in_progress"
            else:
                continue
            cards.append(card(column, item.get("title", item.get("id", "?")),
                              lane=lane_of(data.get("session", "") + " " + str(item.get("branch", ""))),
                              sub=(item.get("note") or item.get("plain") or "")[:160],
                              meta=f"{data.get('session', '')} · {item.get('updated', '')}",
                              ref=item.get("branch") or item.get("id", "")))
    return cards


def needs_you_cards() -> list[dict]:
    """Upstream Dev's open questions to the user: drafts marked as waiting for the user's yes."""
    path = os.path.join(ROOT, "local", "plans", "task-queue.md")
    cards = []
    try:
        text = open(path, encoding="utf-8").read()
    except OSError:
        return cards
    match = re.search(r"^## Waiting on the user.*?$(.*?)(?=^## )", text, re.MULTILINE | re.DOTALL)
    if not match:
        return cards
    for line in match.group(1).splitlines():
        row = re.match(r"^\s*(?:\d+[.)]|[-*]|\|\s*\d+\s*\|)\s*(.+)", line)
        if not row:
            continue
        body = re.sub(r"[*`]", "", row.group(1)).strip(" |")
        if len(body) < 8:
            continue
        cards.append(card("needs_you", body[:110], lane=lane_of(body), sub=body[110:270],
                          meta="task queue", ref=body[:60]))
    return cards


def branch_cards(skip: set[str]) -> list[dict]:
    cards = []
    since = datetime.now() - timedelta(days=7)
    out = run(["git", "for-each-ref", "--sort=-committerdate", "refs/heads",
               "--format=%(refname:short)|%(committerdate:iso-strict)|%(contents:subject)"])
    # Already in portal or upstream main = integrated, not in progress.
    tree = next((b for b in TREE_BRANCHES if run(["git", "branch", "--list", b]).strip()), "plus")
    merged = set(run(["git", "branch", "--format=%(refname:short)", "--merged", tree]).split())
    merged |= set(run(["git", "branch", "--format=%(refname:short)", "--merged", "origin/main"]).split())
    pushed = set(run(["git", "for-each-ref", "--format=%(refname:lstrip=3)", "refs/remotes/myfork"]).split())
    for line in out.splitlines():
        name, date, subject = (line.split("|", 2) + ["", ""])[:3]
        if not BRANCH_RE.match(name) or name in skip or name in merged or name in TREE_BRANCHES:
            continue
        when = datetime.fromisoformat(date).replace(tzinfo=None)
        if when < since:
            continue
        cards.append(card("in_progress", name, lane=lane_of(name), sub=subject[:140],
                          meta=f"last commit {when:%m-%d %H:%M}" + (" · on fork" if name in pushed else ""),
                          ref=name))
    return cards


def build() -> dict:
    heads: set[str] = set()
    cards = github_cards(heads)
    queue_branches: set[str] = set()
    cards += queue_cards(queue_branches)
    cards += draft_cards(heads)
    cards += needs_you_cards()
    cards += roadmap_cards()
    cards += branch_cards(heads | queue_branches)
    return {"generated": datetime.now().strftime("%Y-%m-%d %H:%M"), "columns": COLUMNS, "cards": cards}


WIDGET = """<h2 class="sr-only">Board of Mudcrab upstream work: branches, drafts and pull requests, by stage.</h2>
<style>
.wb-bar{display:flex;flex-wrap:wrap;gap:6px;align-items:center;margin:0 0 12px}
.wb-chip{font-size:12px;padding:3px 10px;border-radius:999px;border:0.5px solid var(--border-strong);background:transparent;color:var(--text-secondary);cursor:pointer}
.wb-chip.on{background:var(--bg-accent);color:var(--text-accent);border-color:var(--border-accent)}
.wb-board{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:10px}
.wb-col{background:var(--surface-1);border-radius:12px;padding:10px;min-width:0}
.wb-col h3{font-size:13px;margin:0 0 8px;display:flex;justify-content:space-between;color:var(--text-primary)}
.wb-card{background:var(--surface-2);border:0.5px solid var(--border);border-radius:8px;padding:8px 10px;margin:0 0 6px;cursor:pointer}
.wb-card:hover{border-color:var(--border-strong)}
.wb-t{font-size:13px;line-height:1.35;color:var(--text-primary);overflow-wrap:anywhere}
.wb-s{font-size:11px;line-height:1.35;color:var(--text-secondary);margin-top:3px;overflow-wrap:anywhere}
.wb-m{font-size:11px;color:var(--text-muted);margin-top:4px;display:flex;gap:6px;flex-wrap:wrap;align-items:center}
.wb-tag{font-size:11px;padding:0 6px;border-radius:999px;background:var(--bg-warning);color:var(--text-warning)}
.wb-more{font-size:12px;width:100%;margin-top:2px}
</style>
<div class="wb-bar" id="wb-bar"><span style="font-size:12px;color:var(--text-muted)" id="wb-gen"></span></div>
<div class="wb-board" id="wb-board"></div>
<script>
const D = __DATA__;
const LANES = ["all", "upstream", "phase 1", "phase 2", "phase 3", "dev"];
let lane = "all"; const open = {};
const bar = document.getElementById("wb-bar"), board = document.getElementById("wb-board");
document.getElementById("wb-gen").textContent = "built " + D.generated + " ·";
const esc = s => String(s).replace(/[&<>"]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"})[c]);
LANES.forEach(l => { const b = document.createElement("button"); b.className = "wb-chip"; b.textContent = l;
  b.onclick = () => { lane = l; draw(); }; b.dataset.lane = l; bar.appendChild(b); });
function draw() {
  bar.querySelectorAll(".wb-chip").forEach(b => b.classList.toggle("on", b.dataset.lane === lane));
  board.innerHTML = "";
  D.columns.forEach(([key, name]) => {
    const cards = D.cards.filter(c => c.column === key && (lane === "all" || c.lane === lane));
    const col = document.createElement("div"); col.className = "wb-col";
    col.innerHTML = `<h3><span>${esc(name)}</span><span style="color:var(--text-muted);font-weight:400">${cards.length}</span></h3>`;
    const limit = open[key] ? cards.length : 6;
    cards.slice(0, limit).forEach(c => {
      const el = document.createElement("div"); el.className = "wb-card";
      el.innerHTML = `<div class="wb-t">${esc(c.title)}</div>` + (c.sub ? `<div class="wb-s">${esc(c.sub)}</div>` : "") +
        `<div class="wb-m"><span>${esc(c.lane)}</span>${c.meta ? `<span>${esc(c.meta)}</span>` : ""}` +
        (c.tag ? `<span class="wb-tag">${esc(c.tag)}</span>` : "") +
        (c.url ? `<a href="${esc(c.url)}" onclick="event.stopPropagation()" aria-label="Open on GitHub"><i class="ti ti-external-link" aria-hidden="true"></i></a>` : "") + `</div>`;
      el.onclick = () => sendPrompt("Board: what is the state of " + c.ref + " and what does it wait on?");
      col.appendChild(el);
    });
    if (cards.length > 6) { const m = document.createElement("button"); m.className = "wb-more";
      m.textContent = open[key] ? "Show fewer" : `Show ${cards.length - 6} more`;
      m.onclick = () => { open[key] = !open[key]; draw(); }; col.appendChild(m); }
    board.appendChild(col);
  });
}
draw();
</script>"""


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--widget", action="store_true")
    args = parser.parse_args()
    board = build()
    if args.json:
        print(json.dumps(board, indent=1, ensure_ascii=False))
    elif args.widget:
        data = json.dumps(board, ensure_ascii=False).replace("</", "<\\/")
        print(WIDGET.replace("__DATA__", data))
    else:
        print(f"work board, built {board['generated']}")
        for key, name in COLUMNS:
            cards = [c for c in board["cards"] if c["column"] == key]
            print(f"\n{name} ({len(cards)})")
            for c in cards:
                print(f"  [{c['lane']}] {c['title'][:90]}" + (f"  ({c['tag']})" if c["tag"] else ""))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
