"""Check a worker's result against the contract in templates/result-schema.json.

A worker answers with one JSON object - base commit, a 300-600 word summary, critical
evidence, unresolved assumptions, the commands it ran, a next falsifying test and the
files it changed. A result can be malformed in ways json.load accepts: a summary of two
sentences, evidence that repeats the claim, a command whose "result" is what it hopes
will happen, an empty changed_files on an implementation task. Those have to be caught
before the lead reads the result, so this checks what a machine can and says what is
wrong in plain lines, one problem per line.

  python tools/check_worker_result.py .ds-result.json
  python tools/check_worker_result.py .ds-result.json --owned tools/a.py,tests/b.py
  python tools/check_worker_result.py .ds-result.json --quiet     # problems only

Passing --owned is what declares the result an implementation result: then changed_files
must be non-empty and every entry has to be one of the assigned files. Without it,
changed_files may be empty, because a research or review result legitimately is.

The input may be the worker's whole answer as the runner saved it, not a bare JSON
document: a ```json fence, prose before or after, and the runner's "session=... status=..."
footer are all tolerated.
"""
import argparse
import json
import re
import sys
from pathlib import Path

# The keys result-schema.json requires, in the order it lists them. A test compares this
# with the schema file, so the two cannot drift apart quietly.
REQUIRED = ('base_commit', 'summary', 'critical_evidence', 'unresolved_assumptions',
            'commands', 'next_falsifying_test', 'changed_files')
EVIDENCE_KEYS = ('claim', 'evidence')
COMMAND_KEYS = ('command', 'result')

MIN_WORDS, MAX_WORDS = 300, 600

# "src/pad_gate.hpp:41", "tools/x.py:3-9", "C:\tree\src\a.c:7", "./tests/a.py:7". A name
# with an extension and a line number. The lookbehind lets a match start after a
# separator, so a Windows path is read whole, while the same shape inside a URL or a
# host name ("example.com:8080") only ever starts after a '.' or a '/', and is dropped.
PATH_LINE = re.compile(r'(?<![\w.:/])[\w./\\-]*[\w-]\.[A-Za-z]\w{0,7}:\d+')
# A path on its own, for evidence that names the file and then says "line 41".
FILE_PATH = re.compile(r'(?<![\w.:/])[\w./\\-]*[\w-]\.[A-Za-z]\w{0,7}\b')
LINE_WORD = re.compile(r'\bline\s*\d+\b', re.I)
# At least two hex digits: "0x4" is an offset a reader cannot follow.
ADDRESS = re.compile(r'\b0x[0-9a-fA-F]{2,}\b')
# A program being run. The name on its own is not a command - "the py side is unchanged"
# names a tool and carries nothing - so it has to be followed by an argument that looks like
# one: a flag ("-m", "--oneline", "/headers"), a path or a file name, a quoted argument. The
# generic names (cat, ls, dir, make, diff) are left out; they are ordinary words.
TOOL = (r'(?:python3?|py|git|cargo|rustc|powershell|pwsh|cmd|nmake|msbuild|dotnet|node|npm|java|gcc|g\+\+|'
        r'clang|cmake|dumpbin|ghidra|unzip|findstr|grep|sed|awk|unittest|curl|wget|7z)')
ARGUMENT = r'(?:[-/."\'$]\S+|[\w-]*[./\\:=][\w./\\=:-]*)'
COMMAND_ARG = re.compile(r'(?<![\w./\\-])%s\s+%s' % (TOOL, ARGUMENT), re.I)
# The tools whose command is a verb of its own: "git status", "npm install". The verb counts
# as an argument for these and for no others, so "the python build failed" stays prose.
SUBCOMMAND_TOOLS = ('git', 'gh', 'cargo', 'npm', 'yarn', 'pip', 'pip3', 'dotnet', 'docker', 'cmake')
SUBCOMMANDS = ('status', 'log', 'diff', 'show', 'blame', 'add', 'commit', 'checkout',
               'branch', 'rev-parse', 'ls-files', 'worktree', 'fetch', 'push', 'pull',
               'clone', 'stash', 'install', 'ci', 'run', 'build', 'publish', 'restore',
               'test', 'check', 'clippy', 'fmt', 'tree', 'nextest')
COMMAND_VERB = re.compile(r'(?<![\w./\\-])(?:%s)\s+(?:%s)\b'
                          % ('|'.join(SUBCOMMAND_TOOLS), '|'.join(SUBCOMMANDS)), re.I)
# A script or binary named without its interpreter, e.g. "tools/round.ps1 Build".
EXECUTABLE = re.compile(r'(?<![\w./\\-])[\w./\\-]+\.(?:py|ps1|psm1|exe|bat|cmd|sh|jar)\b', re.I)
# "will run", "should pass", "expect to", "not yet run": a plan wearing the clothes of a result.
INTENTION = re.compile(r'\b(?:will\s+\w+|should\s+(?:pass|fail|work|run|be)|would\s+\w+|'
                       r'to\s+be\s+(?:run|checked|verified)|plan(?:ned|s)?\s+to|intend(?:ed|s)?\s+to|'
                       r'expect(?:ed|s)?\s+to|not\s+yet|not\s+run|pending|tbd|todo)\b', re.I)
# What output of a run actually looks like: an exit status, the count a run reports, or a plain
# "no output". A result carrying one of these is an outcome even when a sentence in it looks
# ahead - "OK (exit 0); the count will grow as tests are added" - which is why the count and the
# number are asked for by shape: a plan that merely mentions a number ("Will run the 3
# remaining tests.") is still a plan.
OUTCOME = re.compile(r'\bexit(?:ed|s)?\s*[-:=]?\s*\d+'
                     r'|\bran\s+\d+'
                     r'|\d+\s+(?:tests?|failures?|errors?|files?|lines?|handles?|problems?|items?|'
                     r'calls?|matches?|entries?|edges?|warnings?|words?|bytes?)\b'
                     r'|(?:failures|errors)\s*=\s*\d+'
                     r'|\bno\s+(?:output|problems?|matches?|errors?|changes?)\b', re.I)

# Words that answer nothing. Compared whole, after punctuation is flattened, so "N/A."
# and "n/a" and "-" are the same placeholder.
PLACEHOLDERS = {'none', 'na', 'n a', 'nil', 'null', 'tbd', 'todo', 'unknown', 'not applicable',
                'nothing', 'no', 'not sure', 'unclear', 'x'}
# "www.example.com:8080" has the shape of a citation and names nothing in the repository.
HOST_SUFFIXES = frozenset(('com', 'org', 'net', 'edu', 'gov', 'mil', 'int', 'info', 'biz', 'io',
                           'dev', 'co', 'me', 'uk', 'de', 'ru', 'jp', 'fr'))


def type_name(value):
    """How to name a JSON value in a problem line."""
    if value is None:
        return 'null'
    if isinstance(value, bool):
        return 'a boolean'
    if isinstance(value, str):
        return 'a string'
    if isinstance(value, list):
        return 'a list'
    if isinstance(value, dict):
        return 'an object'
    if isinstance(value, (int, float)):
        return 'a number'
    return type(value).__name__


def fenced_blocks(text):
    """Yield the body of every ```...``` block, without its language tag."""
    for match in re.finditer(r'```(.*?)```', text, re.S):
        head, newline, rest = match.group(1).partition('\n')
        if newline and re.fullmatch(r'[ \t]*[A-Za-z0-9_+-]*[ \t]*', head):
            yield rest
        else:
            yield match.group(1)


def brace_objects(text):
    """Yield every outermost {...} run in the text, in the order they appear.

    The scanner respects strings and escapes, so a brace inside a summary does not cut
    the object short. Only outermost runs are yielded: a nested object is part of one.
    """
    start = None
    depth = 0
    in_string = False
    escaped = False
    for index, character in enumerate(text):
        if in_string:
            if escaped:
                escaped = False
            elif character == '\\':
                escaped = True
            elif character == '"':
                in_string = False
            continue
        if character == '"':
            in_string = True
        elif character == '{':
            if depth == 0:
                start = index
            depth += 1
        elif character == '}':
            if depth:
                depth -= 1
                if depth == 0 and start is not None:
                    yield text[start:index + 1]


def load_result(text):
    """Return (data, problems) for the text of a worker's answer.

    data is the parsed value - None when nothing in the text parses. problems says why
    nothing was found, or that what was found is not a JSON object. A bare document, a
    ```json fence, prose around the object and the runner's footer all parse.

    Candidates are tried in three groups, most explicit first: the fenced blocks, then
    the whole text, then the outermost {...} runs. Within a group the object carrying
    the most required keys wins, so a JSON snippet quoted in the worker's prose does not
    beat the real result; a group that produces no object at all is skipped, so a result
    that arrived as a JSON list is read as a list rather than as one of its entries.
    """
    if not isinstance(text, str):
        return None, ['the result text is %s, not a string' % type_name(text)]
    groups = (list(fenced_blocks(text)), [text.strip()], list(brace_objects(text)))

    failure = None
    for group in groups:
        values = []
        for candidate in group:
            candidate = candidate.strip()
            if not candidate:
                continue
            try:
                values.append(json.loads(candidate))
            except ValueError as error:
                if failure is None:
                    failure = error
                continue
        objects = [value for value in values if isinstance(value, dict)]
        if objects:
            return max(objects, key=lambda value: len(set(value) & set(REQUIRED))), []
        if values:
            return values[0], ['the top level is %s, not a JSON object' % type_name(values[0])]
    if failure is None:
        return None, ['no JSON found in the text']
    return None, ['not JSON: %s' % failure]


def type_problem(where, value):
    """Problems for a field that has to be a non-empty string; empty when it is one."""
    if not isinstance(value, str):
        return ['%s is %s, not a string' % (where, type_name(value))]
    if not value.strip():
        return ['%s is empty' % where]
    return []


def check_summary(value):
    """The summary has to be 300-600 whitespace-separated words."""
    problems = type_problem('summary', value)
    if problems:
        return problems
    words = len(value.split())
    if not MIN_WORDS <= words <= MAX_WORDS:
        return ['summary is %d words, outside %d-%d' % (words, MIN_WORDS, MAX_WORDS)]
    return []


def same_text(left, right):
    """True when two fields say the same thing, ignoring case, spacing and punctuation."""
    normal = [re.sub(r'[^a-z0-9]+', ' ', text.lower()).strip() for text in (left, right)]
    first, second = normal
    if not first or not second:
        return False
    if first == second:
        return True
    shorter, longer = sorted((first, second), key=len)
    return len(shorter) >= 0.6 * len(longer) and shorter in longer


def is_placeholder(value):
    """True when the text answers nothing: "none", "N/A.", "-", "TBD", ""."""
    normal = ' '.join(re.findall(r'[a-z0-9]+', value.lower()))
    return not normal or normal in PLACEHOLDERS


def path_line_in(text):
    """True when the text carries file:line - not a host name that only looks like one."""
    for match in PATH_LINE.finditer(text):
        found = match.group(0)
        if '/' in found or '\\' in found:           # a path with a folder is never a host
            return True
        if found.rpartition(':')[0].rsplit('.', 1)[-1].lower() not in HOST_SUFFIXES:
            return True
    return False


def has_reference(text):
    """True when evidence carries something a reader can follow: path:line, 0x..., a command.

    A path without the line number counts when the text also says which line (the repo's
    notes are written both ways), so a real citation is not rejected over punctuation.
    """
    if path_line_in(text) or ADDRESS.search(text):
        return True
    if COMMAND_ARG.search(text) or COMMAND_VERB.search(text) or EXECUTABLE.search(text):
        return True
    return bool(FILE_PATH.search(text) and LINE_WORD.search(text))


def check_evidence(entries):
    """The claims the result rests on, each with evidence that can be followed."""
    if not isinstance(entries, list):
        return ['critical_evidence is %s, not a list' % type_name(entries)]
    if not entries:
        return ['critical_evidence is empty']
    problems = []
    for index, entry in enumerate(entries):
        where = 'critical_evidence[%d]' % index
        if not isinstance(entry, dict):
            problems.append('%s is %s, not an object' % (where, type_name(entry)))
            continue
        for key in EVIDENCE_KEYS:
            if key not in entry:
                problems.append('%s has no %s' % (where, key))
        for key in entry:
            if key not in EVIDENCE_KEYS:
                problems.append('%s has an unexpected key: %s' % (where, key))
        claim, evidence = entry.get('claim'), entry.get('evidence')
        if 'claim' in entry:
            problems += type_problem(where + '.claim', claim)
        if 'evidence' not in entry:
            continue
        problems += type_problem(where + '.evidence', evidence)
        if not isinstance(evidence, str) or not evidence.strip():
            continue
        # One problem per entry: evidence that repeats the claim is not also reported
        # as prose, or a single empty entry would draw two lines.
        if isinstance(claim, str) and same_text(claim, evidence):
            problems.append('%s.evidence only repeats the claim' % where)
        elif not has_reference(evidence):
            problems.append('%s.evidence is only prose: %r' % (where, short(evidence)))
    return problems


def check_commands(entries):
    """The commands the worker ran, each with what it actually printed."""
    if not isinstance(entries, list):
        return ['commands is %s, not a list' % type_name(entries)]
    if not entries:
        return ['commands is empty']
    problems = []
    for index, entry in enumerate(entries):
        where = 'commands[%d]' % index
        if not isinstance(entry, dict):
            problems.append('%s is %s, not an object' % (where, type_name(entry)))
            continue
        for key in COMMAND_KEYS:
            if key not in entry:
                problems.append('%s has no %s' % (where, key))
        for key in entry:
            if key not in COMMAND_KEYS:
                problems.append('%s has an unexpected key: %s' % (where, key))
        if 'command' in entry:
            problems += type_problem(where + '.command', entry.get('command'))
        result = entry.get('result')
        if 'result' not in entry:
            continue
        problems += type_problem(where + '.result', result)
        if not isinstance(result, str) or not result.strip():
            continue
        if INTENTION.search(result) and not OUTCOME.search(result):
            problems.append('%s.result reads as a plan, not an outcome: %r'
                            % (where, short(result)))
    return problems


def clean_path(name):
    """A repo-relative path for comparing changed_files against the owned list.

    Case is folded and separators unified: the worker runs on Windows and the lead's
    brief lists the paths with forward slashes.
    """
    return re.sub(r'^(?:\./)+', '', name.strip().replace('\\', '/')).lower()


def check_changed_files(files, owned=None):
    """Every file the worker wrote, inside the files the brief assigned when it lists them."""
    if not isinstance(files, list):
        return ['changed_files is %s, not a list' % type_name(files)]
    problems = []
    for index, name in enumerate(files):
        if not isinstance(name, str) or not name.strip():
            problems.append('changed_files[%d] is %s, not a file name' % (index, type_name(name)))
    if isinstance(owned, str):          # a caller that passed --owned through unparsed
        owned = parse_owned(owned)
    if not files:
        if owned:
            problems.append('changed_files is empty for an implementation result')
        return problems
    if not owned:
        return problems
    allowed = {clean_path(str(name)) for name in owned}
    for index, name in enumerate(files):
        if isinstance(name, str) and name.strip() and clean_path(name) not in allowed:
            problems.append('changed_files[%d] %r is outside the owned files' % (index, name))
    return problems


def check_assumptions(entries):
    """What the worker had to assume. An empty list is allowed; a placeholder is not."""
    if not isinstance(entries, list):
        return ['unresolved_assumptions is %s, not a list' % type_name(entries)]
    problems = []
    for index, entry in enumerate(entries):
        where = 'unresolved_assumptions[%d]' % index
        problems += type_problem(where, entry)
        if isinstance(entry, str) and entry.strip() and is_placeholder(entry):
            problems.append('%s is a placeholder: %r' % (where, short(entry)))
    return problems


def check_falsifying_test(value):
    """The cheapest check that would show the work wrong: said in words, not "n/a"."""
    problems = type_problem('next_falsifying_test', value)
    if problems:
        return problems
    if is_placeholder(value):
        return ['next_falsifying_test is a placeholder: %r' % short(value)]
    return []


def short(text, limit=60):
    """A value quoted in a problem line, cut so one problem stays one line."""
    text = ' '.join(text.split())
    return text if len(text) <= limit else text[:limit - 3] + '...'


def check_result(data, owned=None):
    """Return every problem with a parsed result; an empty list when it is sound.

    `owned` is the brief's file list. Passing it declares an implementation result, so
    changed_files must be non-empty and inside it.
    """
    if not isinstance(data, dict):
        return ['the top level is %s, not a JSON object' % type_name(data)]
    problems = []
    for key in REQUIRED:
        if key not in data:
            problems.append('missing required key: %s' % key)
    for key in data:
        if key not in REQUIRED:
            problems.append('unexpected key: %s' % key)
    if 'summary' in data:
        problems += check_summary(data['summary'])
    if 'critical_evidence' in data:
        problems += check_evidence(data['critical_evidence'])
    if 'commands' in data:
        problems += check_commands(data['commands'])
    if 'changed_files' in data:
        problems += check_changed_files(data['changed_files'], owned)
    if 'next_falsifying_test' in data:
        problems += check_falsifying_test(data['next_falsifying_test'])
    if 'unresolved_assumptions' in data:
        problems += check_assumptions(data['unresolved_assumptions'])
    return problems


def parse_owned(value):
    """The --owned list, or None when it was not given."""
    names = [name.strip() for name in (value or '').split(',') if name.strip()]
    return names or None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('result', help='the worker result, as the runner saved it')
    parser.add_argument('--owned', metavar='a,b',
                        help='the files the brief assigns; passing it declares an implementation '
                             'result, so changed_files must be non-empty and inside the list')
    parser.add_argument('--quiet', action='store_true', help='print only the problems')
    args = parser.parse_args(argv)

    try:
        text = Path(args.result).read_text(encoding='utf-8', errors='replace')
    except OSError as error:
        print('%s: cannot read: %s' % (args.result, error))
        return 1

    data, problems = load_result(text)
    if isinstance(data, dict):
        problems += check_result(data, parse_owned(args.owned))
    for problem in problems:
        print(problem)
    if not args.quiet:
        print('%s: %s' % (args.result, 'ok' if not problems else '%d problem(s)' % len(problems)))
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
