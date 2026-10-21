"""Resolve git conflict hunks by keeping both sides in full (ours, then theirs), never dropping a
line. For the experimental demo branch only (tools/experimental_build.ps1): neighbouring additions
such as module lists and plugin registrations. Check every hunk it touches before using it."""
import re
import sys

for path in sys.argv[1:]:
    text = open(path, encoding='utf-8').read()
    new, n = re.subn(
        r'<<<<<<< [^\n]*\n(.*?)=======\n(.*?)>>>>>>> [^\n]*\n',
        lambda m: m.group(1) + m.group(2),
        text,
        flags=re.S,
    )
    open(path, 'w', encoding='utf-8', newline='').write(new)
    print(f'{path}: {n} hunk(s) kept both')
