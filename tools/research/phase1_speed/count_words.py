"""Count the words of a Markdown note, ignoring table punctuation and code spans.

`wc -w` counts every `|` and standalone `-` of a Markdown table as a word, which
overstates a table-heavy note; this counts tokens that carry letters or digits.

Usage: python tools/research/phase1_speed/count_words.py <file.md>
"""

from __future__ import annotations

import re
import sys

TOKEN = re.compile(r"[A-Za-z0-9][A-Za-z0-9'’._/:-]*")


def main() -> None:
    text = open(sys.argv[1], encoding="utf-8").read()
    words = TOKEN.findall(text)
    print(f"{len(words)} words (minus table punctuation) in {sys.argv[1]}")


if __name__ == "__main__":
    main()
