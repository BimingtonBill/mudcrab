#!/usr/bin/env python3
"""Redaction lint for Tier 3 clean-room specs (docs/process/clean-room-tier3.md).

Usage: python tools/spec_lint.py <spec.md> [more.md ...]
Exit 0 when no finding, 1 otherwise. It only catches obvious leaks; the reviewer still reads the spec.
"""
import re
import sys

RULES = [
    ("hex address or offset", re.compile(r"(?<![\w.])(?:0x[0-9a-fA-F]{3,}|\+0x[0-9a-fA-F]+)\b")),
    ("bare long hex value", re.compile(r"\b[0-9a-fA-F]{8,16}h?\b(?=[\s,.;)]|$)")),
    ("decompiler symbol", re.compile(r"\b(?:FUN|DAT|LAB|PTR|SUB|sub|param|local|uVar|iVar|pcVar|puVar|lVar|fVar|bVar)_?[0-9a-fA-F]{2,}\b")),
    ("code fence", re.compile(r"^\s*```")),
    ("C-like statement", re.compile(r"(?:\bif\s*\(|\bfor\s*\(|\bwhile\s*\(|\breturn\s+[\w(*&-]|->\w|::\w|;\s*$)")),
    ("vtable or RTTI name", re.compile(r"\b(?:vtable|vftable|vfptr|RTTI|\?AV[\w@]+|\.PEAV)\b", re.I)),
    ("struct layout", re.compile(r"\b(?:struct|class)\s+\w+\s*\{|\boffset\s+(?:0x)?[0-9a-fA-F]+\b", re.I)),
    ("registers or calling convention", re.compile(r"\b(?:rax|rbx|rcx|rdx|rsi|rdi|rsp|rbp|eax|ebx|ecx|edx|xmm\d+|__fastcall|__thiscall|__stdcall)\b", re.I)),
    ("assembly mnemonic line", re.compile(r"^\s*(?:mov|lea|call|jmp|jne|je|cmp|push|pop|ret|xor|movss|mulss|addss)\s+\w", re.I)),
    ("address-library or tool name", re.compile(r"\b(?:Ghidra|IDA|decompil\w*|disassembl\w*)\b.*\b(?:shows|output|listing)\b", re.I)),
]


def lint(path):
    findings = []
    with open(path, encoding="utf-8") as handle:
        for number, line in enumerate(handle, 1):
            for name, pattern in RULES:
                if pattern.search(line):
                    findings.append((number, name, line.strip()[:120]))
    return findings


def main(paths):
    if not paths:
        print(__doc__)
        return 2
    failed = False
    for path in paths:
        findings = lint(path)
        for number, name, text in findings:
            print(f"{path}:{number}: {name}: {text}")
        if findings:
            failed = True
        else:
            print(f"{path}: ok (no obvious leaks; a reviewer must still read it)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
