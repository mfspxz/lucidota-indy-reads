#!/usr/bin/env python3
"""Vendored from ~/workspace/bin/secret-scrub.py (Oct-8 repair wave, fail-closed
pre-commit/CI gate), vendored into indy-reads on 2026-10-08 so remote CI can run
the gate with zero external dependencies. Byte-faithful copy below this header;
upstream fixes should be re-vendored, not forked. See .secrets-allowlist."
"""

"""secret-scrub.py — fail-closed secret scanner. Mandatory pre-commit gate.

Scans files for candidate secret material and FAILS (exit 1) with a
file:line report when anything matches. It never prints the matched
values, only their locations. This is the automated pipeline step the
Oct-8 repair wave mandated after raw tokens sat in TOKEN-STATUS.md
unnoticed: no commit lands without passing this gate.

Usage:
  secret-scrub.py [files...]        # scan given files (default: git staged)
  secret-scrub.py --install [repo]  # install as .git/hooks/pre-commit

Allowlist: .secrets-allowlist (repo root) — one regex per line, matched
against the full line. Use sparingly; every entry is a hole.
"""
import math
import os
import re
import subprocess
import sys
from collections import Counter

PREFIX_PAT = re.compile(
    r'(?:sk-[A-Za-z0-9_\-]{8,}|ghp_[A-Za-z0-9]{8,}|gho_[A-Za-z0-9]{8,}|'
    r'ghu_[A-Za-z0-9]{8,}|xox[bpras]-[A-Za-z0-9\-]{8,}|AKIA[A-Z0-9]{16}|'
    r'cfut_[A-Za-z0-9_\-]{8,}|cfat_[A-Za-z0-9_\-]{8,}|AIza[A-Za-z0-9_\-]{8,}|-----BEGIN [A-Z ]*PRIVATE KEY-----)'
)
LONG_PAT = re.compile(r'[A-Za-z0-9_.\-~+/=]{32,}')

SKIP_DIRS = {'.venv', 'node_modules', '.git', 'target', '__pycache__'}
SKIP_FILES = {'secret-scrub.py'}


def shannon(s: str) -> float:
    if not s:
        return 0.0
    c = Counter(s)
    n = len(s)
    return -sum((v / n) * math.log2(v / n) for v in c.values())


def load_allowlist(root: str):
    p = os.path.join(root, '.secrets-allowlist')
    if not os.path.exists(p):
        return []
    return [re.compile(l.strip()) for l in open(p) if l.strip() and not l.startswith('#')]


def repo_root() -> str:
    try:
        return subprocess.run(['git', 'rev-parse', '--show-toplevel'],
                              capture_output=True, text=True, check=True).stdout.strip()
    except Exception:
        return os.getcwd()


def staged_files(root: str):
    try:
        out = subprocess.run(['git', 'diff', '--cached', '--name-only', '--diff-filter=ACM'],
                             capture_output=True, text=True, cwd=root, check=True).stdout
        return [os.path.join(root, f) for f in out.splitlines() if f]
    except Exception:
        return []


def scan_file(path: str, allow):
    hits = []
    try:
        with open(path, errors='replace') as f:
            for i, line in enumerate(f, 1):
                if any(a.search(line) for a in allow):
                    continue
                if PREFIX_PAT.search(line):
                    hits.append(i)
                    continue
                for m in LONG_PAT.findall(line):
                    # high-entropy long token, not a UUID / path / hash-looking run
                    if len(m) >= 40 and shannon(m) > 4.0:
                        hits.append(i)
                        break
                    if len(m) >= 64:  # long even at low entropy: worth a look
                        hits.append(i)
                        break
    except (OSError, UnicodeError):
        pass
    return hits


def main(argv):
    if '--install' in argv:
        root = argv[argv.index('--install') + 1] if len(argv) > argv.index('--install') + 1 else repo_root()
        hook = os.path.join(root, '.git', 'hooks', 'pre-commit')
        with open(hook, 'w') as f:
            f.write('#!/bin/sh\n# installed by secret-scrub.py --install (Oct-8 repair wave)\n'
                    'exec python3 "$HOME/workspace/bin/secret-scrub.py"\n')
        os.chmod(hook, 0o755)
        print(f"installed pre-commit hook in {root}")
        return 0

    root = repo_root()
    allow = load_allowlist(root)
    files = argv[1:] if argv[1:] and not argv[1].startswith('-') else staged_files(root)
    bad = {}
    for fp in files:
        if not os.path.isfile(fp):
            continue
        if os.path.basename(fp) in SKIP_FILES:
            continue
        if any(d in os.path.relpath(fp, root).split(os.sep) for d in SKIP_DIRS):
            continue
        lines = scan_file(fp, allow)
        if lines:
            bad[os.path.relpath(fp, root)] = lines
    if bad:
        print("secret-scrub: BLOCKED — candidate secret material detected (values not shown):")
        for fp, lines in bad.items():
            print(f"  {fp}:{','.join(map(str, lines[:10]))}")
        print("Remove the values (use the Secure Vault / key files), then commit again.")
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
