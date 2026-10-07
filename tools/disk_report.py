#!/usr/bin/env python3
"""Read-only disk report for the shared Windows dev machine.

Several sessions create build folders, git worktrees, evidence copies and
scratch folders under C:\\Modding and D:\\Modding-overflow. This script lists
them with owner, class, size and expiry so the owner can clean up.

THIS SCRIPT NEVER DELETES, MOVES, RENAMES OR WRITES ANYTHING (except the
optional --out report file). Safety matters more than completeness: anything
that is not clearly safe is listed under "Keep", "Who owns this?" or
"Class C", never under "Can go".

Each managed folder carries an OWNER.txt marker (print one with --template).
Python 3.13, standard library only.
"""

from __future__ import annotations

import argparse
import fnmatch
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import time
from dataclasses import dataclass, field
from datetime import date, datetime, timedelta, timezone

# ---------------------------------------------------------------------------
# Constants (easy to extend)
# ---------------------------------------------------------------------------

DEFAULT_ROOTS = [r"C:\Modding", r"D:\Modding-overflow"]

# Folders whose children (not the folder itself) are the entries.
PARENT_DIR_NAMES = ["wt", "wt-targets", "worktrees", "builds"]

# NEVER-TOUCH list: folder names (case-insensitive, fnmatch patterns). These
# are listed by name only and never walked or sized. Add new names here.
NEVER_TOUCH_PATTERNS = [
    "Bethini Pie*",
    "Modlists",
    "Wabbajack",
    "MudcrabProject",  # the main checkout
    "SkyrimConverted*",  # converted game data; measuring it once blocked a conversion
    ".cargo",  # C:\Modding\.cargo holds the quick build profile for every checkout
    "wkj-contributor-status",  # the user's local-only contributor status tool (old path)
    "contributor-status",  # the same tool under C:\Modding\worktrees since 2026-10-07
    "OpenSkyrimProject",  # the main checkout (folder name on this PC)
]

DRIVES = ["C:\\", "D:\\"]
OWNER_FILE = "OWNER.txt"
OWNER_FILE_MAX_BYTES = 64 * 1024
PR_REPO = "Mudcrab-Team/mudcrab"
DEFAULT_TIME_LIMIT = 60.0
DEFAULT_GRACE_DAYS = 3
SUBPROCESS_TIMEOUT = 120
FILE_ATTRIBUTE_REPARSE_POINT = 0x400
GB = 1024**3

# Free-space levels shared with the other projects on this PC
# (D:\VFXEnhance\coordination\RESOURCES.md; tools/disk_guard.py uses the same).
FLOOR_GB = 10.0
WARN_GB = 15.0

# dust (fast, read-only sizing; counts each hard-linked file once). Used only
# with --size-protected, for folders this report never walks itself.
DUST_CANDIDATES = [
    os.environ.get("DUST", ""),
    os.path.expandvars(
        r"%LOCALAPPDATA%\Microsoft\WinGet\Packages"
        r"\bootandy.dust_Microsoft.Winget.Source_8wekyb3d8bbwe"
        r"\dust-v1.2.5-x86_64-pc-windows-gnu\dust.exe"
    ),
    "dust",
]
# Sizing these while a conversion publishes can make the publish fail
# ("Access is denied"), so --size-protected skips them while one runs.
CONVERTER_PROCESSES = ["converter.exe", "launcher.exe"]
DUST_UNITS = {"B": 1, "K": 1024, "M": 1024**2, "G": 1024**3, "T": 1024**4}

# Report sections.
CAN_GO = "can_go"
KEEP = "keep"
UNKNOWN = "unknown"
CLASS_C = "class_c"
NEVER = "never"

TEMPLATE = """\
# Put this file in the folder as OWNER.txt. One key: value per line.
owner: Phase 2 Dev
purpose: build folder for phase2/* checks
class: A            # A = rebuildable (build targets, scratch), B = evidence/data copy, C = converted game data or anything with links
keep-until: pr:125  # or date:2026-10-09, or while-open (never expires by itself)
links: no           # yes if it contains junctions/symlinks/hard links into other folders
created: 2026-10-02
"""

FOOTER = (
    "This report deletes nothing. Only the owner deletes its own folders, "
    "with the user's yes; worktrees via `git worktree remove` (never --force)."
)


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------


@dataclass
class WalkResult:
    """What a size walk found."""

    size: int = 0
    files: int = 0
    hardlinked_files: int = 0  # files seen with more than one hard link
    links_found: str | None = None  # relative path of the first junction/symlink
    timed_out: bool = False
    errors: int = 0  # paths that could not be read


@dataclass
class Entry:
    """One row of the report."""

    path: str
    section: str
    owner: str = "-"
    cls: str = "-"
    keep_until: str = "-"
    size: int | None = None
    timed_out: bool = False
    reason: str = ""


@dataclass
class Options:
    """Settings for one report run."""

    time_limit: float = DEFAULT_TIME_LIMIT
    grace_days: int = DEFAULT_GRACE_DAYS
    now: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    today: date = field(default_factory=date.today)


# ---------------------------------------------------------------------------
# OWNER.txt
# ---------------------------------------------------------------------------


def strip_comment(value: str) -> str:
    """Remove a trailing ' # comment' from a value and trim spaces."""
    return re.split(r"\s+#", value, maxsplit=1)[0].strip()


def parse_owner_text(text: str) -> tuple[dict | None, str]:
    """Parse OWNER.txt text. Return (info, problem); info is None if unusable.

    Unknown keys and comment lines are ignored. The file is unusable when
    `owner` is missing or empty, `class` is not A, B or C, or `links` is
    given but is not yes/no.
    """
    raw: dict[str, str] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#") or ":" not in line:
            continue
        key, value = line.split(":", 1)
        key = key.strip().lower().replace("_", "-")
        raw.setdefault(key, strip_comment(value))
    owner = raw.get("owner", "")
    if not owner:
        return None, "OWNER.txt has no owner line"
    cls = raw.get("class", "").upper()
    if cls not in ("A", "B", "C"):
        return None, f"OWNER.txt class is not A, B or C: {raw.get('class', '')!r}"
    links = raw.get("links", "no").lower()
    if links not in ("yes", "no"):
        return None, f"OWNER.txt links is not yes/no: {raw.get('links', '')!r}"
    keep_until = raw.get("keep-until", "").lower() or "while-open"
    return (
        {
            "owner": owner,
            "purpose": raw.get("purpose", ""),
            "class": cls,
            "keep-until": keep_until,
            "links": links == "yes",
            "created": raw.get("created", ""),
        },
        "",
    )


def read_owner(folder: str) -> tuple[dict | None, str]:
    """Read and parse <folder>\\OWNER.txt. Return (info, problem)."""
    # Converted data keeps its note beside the folder (<folder>.OWNER.txt): a file inside a converted
    # output makes a later --reuse-assets rebuild refuse it ("retained asset has no manifest provenance").
    path = os.path.join(folder, OWNER_FILE)
    try:
        st = os.lstat(path)
    except OSError:
        path = os.path.normpath(folder) + "." + OWNER_FILE
        try:
            st = os.lstat(path)
        except OSError:
            return None, "no OWNER.txt"
    if not stat.S_ISREG(st.st_mode) or st.st_size > OWNER_FILE_MAX_BYTES:
        return None, "OWNER.txt is not a normal small file"
    try:
        with open(path, encoding="utf-8-sig", errors="replace") as handle:
            text = handle.read()
    except OSError as exc:
        return None, f"OWNER.txt unreadable: {exc}"
    return parse_owner_text(text)


# ---------------------------------------------------------------------------
# Expiry
# ---------------------------------------------------------------------------


def fetch_pr(number: int) -> dict | None:
    """Ask gh (read-only) for a PR's state. Return the JSON dict or None."""
    cmd = [
        "gh", "pr", "view", str(number), "--repo", PR_REPO,
        "--json", "state,mergedAt,closedAt",
    ]  # fmt: skip
    try:
        done = subprocess.run(
            cmd, capture_output=True, text=True, timeout=SUBPROCESS_TIMEOUT
        )
        if done.returncode != 0:
            return None
        data = json.loads(done.stdout)
    except (OSError, subprocess.SubprocessError, ValueError):
        return None
    return data if isinstance(data, dict) else None


def parse_timestamp(value: str | None) -> datetime | None:
    """Parse an ISO timestamp from gh (e.g. 2026-10-01T12:00:00Z), or None."""
    if not value:
        return None
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed


def check_pr_expiry(
    number: int, cls: str, opts: Options, pr_lookup
) -> tuple[bool, str]:
    """Expiry of a `pr:N` keep-until. Class A: merged/closed. Class B: grace days after."""
    data = pr_lookup(number)
    if not data:
        return False, f"PR #{number} state unknown (gh failed)"
    state = str(data.get("state", "")).upper()
    if state not in ("MERGED", "CLOSED"):
        return False, f"PR #{number} is {state.lower() or 'in an unknown state'}"
    if cls == "A":
        return True, f"PR #{number} {state.lower()}"
    stamp = parse_timestamp(data.get("mergedAt")) or parse_timestamp(data.get("closedAt"))
    if stamp is None:
        return False, f"PR #{number} {state.lower()} but no date known"
    due = stamp + timedelta(days=opts.grace_days)
    if opts.now >= due:
        return True, f"PR #{number} {state.lower()} over {opts.grace_days} days ago"
    return False, (
        f"PR #{number} {state.lower()}; class B grace until {due.date().isoformat()}"
    )


def check_expiry(info: dict, opts: Options, pr_lookup=None) -> tuple[bool, str]:
    """Return (expired, note) for an OWNER.txt info dict.

    date:YYYY-MM-DD expires after that day; pr:N follows the PR (class A when
    merged/closed, class B after the grace days); while-open or anything not
    understood never expires.
    """
    pr_lookup = pr_lookup or fetch_pr
    value = info["keep-until"]
    if value == "while-open":
        return False, "keep-until while-open (never expires by itself)"
    if value.startswith("date:"):
        try:
            limit = date.fromisoformat(value[5:].strip())
        except ValueError:
            return False, f"keep-until date not understood: {value}"
        if opts.today > limit:
            return True, f"expired on {limit.isoformat()}"
        return False, f"not expired until after {limit.isoformat()}"
    match = re.fullmatch(r"pr:\s*(\d+)", value)
    if match:
        return check_pr_expiry(int(match.group(1)), info["class"], opts, pr_lookup)
    return False, f"keep-until not understood ({value}); treated as never expires"


# ---------------------------------------------------------------------------
# Walking (in-process, never follows links)
# ---------------------------------------------------------------------------


def attrs_are_reparse(st: os.stat_result) -> bool:
    """True when Windows file attributes mark a reparse point (junction, symlink, ...)."""
    return bool(getattr(st, "st_file_attributes", 0) & FILE_ATTRIBUTE_REPARSE_POINT)


def path_is_link(path: str) -> bool:
    """True if `path` itself is a symlink, junction or other reparse point."""
    try:
        if os.path.islink(path) or os.path.isjunction(path):
            return True
        return attrs_are_reparse(os.lstat(path))
    except OSError:
        return False


def entry_is_link(entry: os.DirEntry) -> bool:
    """True if a directory entry is a symlink, junction or other reparse point."""
    if entry.is_symlink() or entry.is_junction():
        return True
    return attrs_are_reparse(entry.stat(follow_symlinks=False))


def walk_tree(root: str, time_limit: float, clock=time.monotonic) -> WalkResult:
    """Size a folder with os.scandir without ever following links.

    Stops at the first junction/symlink (links_found is then set and the size
    must not be used). Hard-linked files are counted once, by (st_dev, st_ino).
    Stops when `time_limit` seconds have passed (timed_out is set; the size is
    then only a lower bound). Runs in this process, nothing keeps running after.
    """
    result = WalkResult()
    deadline = clock() + time_limit
    seen_links: set[tuple[int, int]] = set()
    pending = [root]
    while pending:
        directory = pending.pop()
        try:
            iterator = os.scandir(directory)
        except OSError:
            result.errors += 1
            continue
        with iterator:
            for entry in iterator:
                if clock() > deadline:
                    result.timed_out = True
                    return result
                if not scan_entry(entry, root, result, pending, seen_links):
                    return result  # a link was found
    return result


def scan_entry(entry, root, result, pending, seen_links) -> bool:
    """Handle one directory entry. Return False if a link was found (stop)."""
    try:
        if entry_is_link(entry):
            result.links_found = os.path.relpath(entry.path, root)
            return False
        if entry.is_dir(follow_symlinks=False):
            pending.append(entry.path)
            return True
        st = os.lstat(entry.path)  # scandir's stat has no inode/link count on Windows
        if attrs_are_reparse(st):
            result.links_found = os.path.relpath(entry.path, root)
            return False
    except OSError:
        result.errors += 1
        return True
    result.files += 1
    if st.st_nlink > 1:
        result.hardlinked_files += 1
        key = (st.st_dev, st.st_ino)
        if key in seen_links:
            return True
        seen_links.add(key)
    result.size += st.st_size
    return True


# ---------------------------------------------------------------------------
# Git worktrees (read-only)
# ---------------------------------------------------------------------------


def git_kind(folder: str) -> str | None:
    """'worktree' if <folder>\\.git is a file, 'clone' if it is a folder, else None."""
    dot_git = os.path.join(folder, ".git")
    if path_is_link(dot_git):
        return None
    if os.path.isfile(dot_git):
        return "worktree"
    if os.path.isdir(dot_git):
        return "clone"
    return None


def run_git(folder: str, *args: str) -> str | None:
    """Run a read-only git command in `folder`; return stdout or None on failure."""
    env = dict(os.environ, GIT_OPTIONAL_LOCKS="0")
    cmd = ["git", "--no-optional-locks", "-C", folder, *args]
    try:
        done = subprocess.run(
            cmd, capture_output=True, text=True, timeout=SUBPROCESS_TIMEOUT, env=env
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return done.stdout if done.returncode == 0 else None


def is_work_line(line: str) -> bool:
    """A `git status --porcelain` line that is real work, not the folder's own untracked OWNER.txt."""
    return bool(line.strip()) and line.strip() != "?? OWNER.txt"


def worktree_info(folder: str) -> dict:
    """Branch and number of uncommitted/untracked entries (None when git failed)."""
    status = run_git(folder, "status", "--porcelain")
    branch = run_git(folder, "rev-parse", "--abbrev-ref", "HEAD")
    dirty = None if status is None else len([ln for ln in status.splitlines() if is_work_line(ln)])
    return {"branch": (branch or "").strip() or "?", "dirty": dirty}


# ---------------------------------------------------------------------------
# Building entries
# ---------------------------------------------------------------------------


def is_never_touch(name: str) -> bool:
    """True if a folder name is on the never-touch list."""
    lowered = name.lower()
    return any(fnmatch.fnmatch(lowered, pat.lower()) for pat in NEVER_TOUCH_PATTERNS)


def list_subdirs(parent: str) -> list[str]:
    """Full paths of the sub-folders of `parent` (links to folders included, not followed)."""
    found = []
    try:
        with os.scandir(parent) as iterator:
            for entry in iterator:
                if entry.is_dir():  # follows a link only to tell it is a folder
                    found.append(entry.path)
    except OSError as exc:
        print(f"warning: cannot list {parent}: {exc}", file=sys.stderr)
    return sorted(found, key=str.lower)


def discover(roots: list[str]) -> list[str]:
    """Candidate folders: root children, with wt / wt-targets replaced by their children."""
    paths: list[str] = []
    seen: set[str] = set()
    for root in roots:
        for path in list_subdirs(root):
            name = os.path.basename(path).lower()
            if name in PARENT_DIR_NAMES and not path_is_link(path):
                children = list_subdirs(path)
            else:
                children = [path]
            for child in children:
                key = os.path.normcase(os.path.abspath(child))
                if key not in seen:
                    seen.add(key)
                    paths.append(child)
    return paths


def make_class_c(path: str, info: dict, reason: str) -> Entry:
    """A class C / links entry: listed, never walked or sized."""
    return Entry(
        path, CLASS_C, info["owner"], info["class"], info["keep-until"], reason=reason
    )


def describe_walk(walk: WalkResult, blockers: list[str], notes: list[str]) -> None:
    """Add hard-link and read-error remarks from a walk to the note lists."""
    if walk.hardlinked_files:
        notes.append(
            f"{walk.hardlinked_files} hard-linked files: deleting frees less than shown"
        )
    if walk.errors:
        blockers.append(f"{walk.errors} paths unreadable: size incomplete")
    if walk.timed_out:
        blockers.append(
            "walk timed out: not fully scanned for links, so not offered "
            "(rerun with a longer --time-limit)"
        )


def describe_git(path: str, blockers: list[str], notes: list[str]) -> None:
    """Add git worktree remarks (branch, uncommitted changes) to the note lists."""
    kind = git_kind(path)
    if kind is None:
        return
    info = worktree_info(path)
    if kind == "clone":
        blockers.append("is a full git clone: unpushed commits not checked")
        return
    notes.append(f"worktree on branch {info['branch']}")
    if info["dirty"] is None:
        blockers.append("git status failed: cannot tell if work is uncommitted")
    elif info["dirty"] > 0:
        blockers.append(
            f"has uncommitted changes ({info['dirty']} entries): "
            "owner must commit or discard first"
        )


def process_managed(path: str, info: dict, opts: Options, pr_lookup) -> Entry:
    """Walk and judge a class A or B folder that declares no links."""
    walk = walk_tree(path, opts.time_limit)
    if walk.links_found:
        reason = f"contains links -> treat as class C (found {walk.links_found})"
        if not info["links"]:
            reason += "; OWNER.txt said links: no"
        return make_class_c(path, info, reason)
    blockers: list[str] = []
    notes: list[str] = []
    describe_walk(walk, blockers, notes)
    describe_git(path, blockers, notes)
    expired, why = check_expiry(info, opts, pr_lookup)
    section = CAN_GO if expired and not blockers else KEEP
    reason = "; ".join(blockers + [why] + notes)
    return Entry(
        path, section, info["owner"], info["class"], info["keep-until"],
        walk.size, walk.timed_out, reason,
    )  # fmt: skip


def process_entry(path: str, opts: Options, pr_lookup) -> Entry:
    """Decide the section for one candidate folder (see the module docstring)."""
    if is_never_touch(os.path.basename(path)):
        return Entry(path, NEVER, reason="never touched: listed by name only")
    if path_is_link(path):
        return Entry(path, CLASS_C, reason="folder is itself a junction/symlink; not read")
    info, problem = read_owner(path)
    if info is None:
        return Entry(path, UNKNOWN, reason=f"unknown owner ({problem}); not sized")
    if info["class"] == "C" or info["links"]:
        why = "class C" if info["class"] == "C" else "owner declared links: yes"
        return make_class_c(path, info, f"{why}; not walked or sized")
    return process_managed(path, info, opts, pr_lookup)


def build_report(roots: list[str], opts: Options) -> list[Entry]:
    """Scan all roots and return the report entries."""
    cache: dict[int, dict | None] = {}

    def pr_lookup(number: int) -> dict | None:
        if number not in cache:
            cache[number] = fetch_pr(number)
        return cache[number]

    return [process_entry(path, opts, pr_lookup) for path in discover(roots)]


def find_dust() -> str | None:
    """Path of a usable dust executable, or None."""
    for candidate in DUST_CANDIDATES:
        if not candidate:
            continue
        found = shutil.which(candidate) or (candidate if os.path.isfile(candidate) else None)
        if found:
            return found
    return None


def parse_dust_size(text: str) -> int | None:
    """'14Gi' or '512Mi' or '0B' -> bytes."""
    match = re.fullmatch(r"\s*([0-9.]+)\s*([BKMGT])?i?B?\s*", text)
    if not match:
        return None
    return int(float(match.group(1)) * DUST_UNITS[match.group(2) or "B"])


def dust_size(dust: str, path: str) -> int | None:
    """Total size of one folder from `dust -j -d 0`, or None on any failure."""
    try:
        done = subprocess.run(
            [dust, "-j", "-d", "0", path],
            capture_output=True,
            text=True,
            timeout=SUBPROCESS_TIMEOUT,
        )
        return parse_dust_size(json.loads(done.stdout)["size"])
    except (OSError, subprocess.SubprocessError, ValueError, KeyError, TypeError):
        return None


def converter_running() -> bool:
    """True when a converter or launcher process is running (Windows tasklist)."""
    try:
        done = subprocess.run(["tasklist", "/fo", "csv", "/nh"], capture_output=True, text=True,
                              timeout=SUBPROCESS_TIMEOUT)  # fmt: skip
    except (OSError, subprocess.SubprocessError):
        return True  # unknown: assume one runs, so nothing is sized
    names = done.stdout.lower()
    return any(f'"{name}"' in names for name in CONVERTER_PROCESSES)


def size_protected(entries: list[Entry]) -> str:
    """Fill in dust sizes for never-touch and class C entries; return a note."""
    dust = find_dust()
    if dust is None:
        return "Protected folders not sized: dust not found (set DUST to its path)."
    if converter_running():
        return "Protected folders not sized: a converter or launcher is running."
    for entry in entries:
        if entry.section in (NEVER, CLASS_C) and entry.size is None:
            entry.size = dust_size(dust, entry.path)
            if entry.size is not None and entry.reason:
                entry.reason = entry.reason.replace("not walked or sized", "not walked; size from dust")
    return "Protected folders sized with dust (read-only; never offered for deletion)."


# ---------------------------------------------------------------------------
# Rendering
# ---------------------------------------------------------------------------

SECTION_TITLES = [
    (CAN_GO, "Can go (owner should delete, with the user's yes)"),
    (KEEP, "Keep"),
    (UNKNOWN, "Who owns this?"),
    (CLASS_C, "Class C (own decision, never routine)"),
    (NEVER, "Never-touch"),
]


def format_size(entry: Entry) -> str:
    """Size in GB with one decimal, '>= X GB (timed out)' or 'not sized'."""
    if entry.size is None:
        return "not sized"
    text = f"{entry.size / GB:.1f} GB"
    return f">= {text} (timed out)" if entry.timed_out else text


def cell(text: str) -> str:
    """Make text safe inside a Markdown table cell."""
    return str(text).replace("|", "\\|").replace("\r", " ").replace("\n", " ")


def render_table(entries: list[Entry]) -> list[str]:
    """Markdown table lines for a list of entries."""
    lines = [
        "| Path | Owner | Class | Size | Keep-until | State / reason |",
        "|---|---|---|---|---|---|",
    ]
    for e in entries:
        cols = [e.path, e.owner, e.cls, format_size(e), e.keep_until, e.reason]
        lines.append("| " + " | ".join(cell(c) for c in cols) + " |")
    return lines


def render_never(entries: list[Entry]) -> list[str]:
    """Never-touch folders by name, with a dust size when --size-protected ran."""
    return [f"- {e.path}" + (f" ({format_size(e)}, dust)" if e.size is not None else "") for e in entries]


def drive_summary() -> str:
    """Free space per drive, e.g. 'C: 12.3 GB free of 500.0 GB'."""
    parts = []
    for drive in DRIVES:
        try:
            usage = shutil.disk_usage(drive)
            free = usage.free / GB
            level = ""
            if free < FLOOR_GB:
                level = f" (BELOW THE {FLOOR_GB:.0f} GB FLOOR)"
            elif free < WARN_GB:
                level = f" (below {WARN_GB:.0f} GB: warn the other sessions)"
            parts.append(f"{drive[:2]} {free:.1f} GB free of {usage.total / GB:.1f} GB{level}")
        except OSError:
            parts.append(f"{drive[:2]} not available")
    return "; ".join(parts)


def render_report(entries: list[Entry], now: datetime | None = None, note: str = "") -> str:
    """The whole Markdown report."""
    now = now or datetime.now()
    groups = {key: [e for e in entries if e.section == key] for key, _ in SECTION_TITLES}
    for key in (CAN_GO, KEEP):
        groups[key].sort(key=lambda e: e.size or 0, reverse=True)
    counts = ", ".join(f"{title.split(' (')[0].lower()} {len(groups[key])}"
                       for key, title in SECTION_TITLES)  # fmt: skip
    lines = [
        "# Disk report",
        "",
        f"Generated {now.strftime('%Y-%m-%d %H:%M')}. Free space: {drive_summary()}.",
        f"Entries: {counts}.",
    ]
    if note:
        lines.append(note)
    for key, title in SECTION_TITLES:
        lines += ["", f"## {title}", ""]
        rows = groups[key]
        if not rows:
            lines.append("(none)")
        elif key == NEVER:
            lines += render_never(rows)
        else:
            lines += render_table(rows)
            if key == CAN_GO:
                total = sum(e.size or 0 for e in rows)
                lines += ["", f"Total listed: {total / GB:.1f} GB (hard links may free less)."]
    lines += ["", FOOTER, ""]
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Command line
# ---------------------------------------------------------------------------


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    """Command line options."""
    parser = argparse.ArgumentParser(description="Read-only disk report (deletes nothing).")
    parser.add_argument("--root", action="append", help="root folder to scan (repeatable)")
    parser.add_argument("--time-limit", type=float, default=DEFAULT_TIME_LIMIT,
                        help="seconds per folder walk (default 60)")  # fmt: skip
    parser.add_argument("--grace-days", type=int, default=DEFAULT_GRACE_DAYS,
                        help="class B grace days after a PR is merged/closed (default 3)")  # fmt: skip
    parser.add_argument("--out", help="also write the report to this file")
    parser.add_argument("--template", action="store_true", help="print an OWNER.txt template")
    parser.add_argument("--size-protected", action="store_true",
                        help="size never-touch and class C folders with dust (skipped while a "
                        "converter or launcher runs)")  # fmt: skip
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    """Entry point."""
    args = parse_args(argv)
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(errors="replace")
    if args.template:
        print(TEMPLATE, end="")
        return 0
    opts = Options(time_limit=args.time_limit, grace_days=args.grace_days)
    entries = build_report(args.root or DEFAULT_ROOTS, opts)
    note = size_protected(entries) if args.size_protected else ""
    text = render_report(entries, note=note)
    print(text)
    if args.out:
        with open(args.out, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
