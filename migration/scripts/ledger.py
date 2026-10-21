#!/usr/bin/env python3
"""The work log: migration/LEDGER.md (frozen history) plus one file per entry
in migration/ledger/YYYY-MM-DD-<slug>.md, so concurrent PRs never edit the
same file.

  python3 migration/scripts/ledger.py           print every entry in date order
  python3 migration/scripts/ledger.py --list    list the entry files
  python3 migration/scripts/ledger.py --check   validate entry names and headings
  python3 migration/scripts/ledger.py --check-frozen
                                                fail if LEDGER.md changed

Each entry file starts with a "## YYYY-MM-DD: <title>" heading whose date
matches its file name. Entries are ordered by file name (date, then slug).
The reading order is mirrored in port/crates/parity/src/cases.rs
(read_ledger). Python 3.8+ standard library only.
"""

import argparse
import hashlib
import os
import re
import sys

ENTRY_NAME = re.compile(r"(\d{4}-\d{2}-\d{2})-[a-z0-9]+(?:-[a-z0-9]+)*\.md")
FROZEN = os.path.join("migration", "LEDGER.md")
ENTRIES = os.path.join("migration", "ledger")
# SHA-256 of the frozen migration/LEDGER.md.
FROZEN_SHA256 = "81b592f7d916d365aeb0aebeafc1c1b7ecf0d1e96c2ea46f21ccf046514d06fc"


def default_repo():
    return os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))


def entry_files(repo):
    """Entry file names in migration/ledger/, in date order (README.md and
    other non-entry files are not entries)."""
    directory = os.path.join(repo, ENTRIES)
    if not os.path.isdir(directory):
        return []
    return sorted(name for name in os.listdir(directory) if name.endswith(".md") and name != "README.md")


def read_text(repo):
    """The whole ledger as one markdown text: LEDGER.md, then every entry."""
    parts = []
    frozen = os.path.join(repo, FROZEN)
    if os.path.exists(frozen):
        with open(frozen) as handle:
            parts.append(handle.read())
    for name in entry_files(repo):
        with open(os.path.join(repo, ENTRIES, name)) as handle:
            parts.append(handle.read())
    return "\n\n".join(part.rstrip("\n") for part in parts) + "\n"


def problems(repo):
    found = []
    for name in entry_files(repo):
        match = ENTRY_NAME.fullmatch(name)
        if not match:
            found.append("%s/%s: name must be YYYY-MM-DD-<slug>.md (slug: lowercase letters, digits, -)"
                         % (ENTRIES, name))
            continue
        with open(os.path.join(repo, ENTRIES, name)) as handle:
            first = next((line.rstrip("\n") for line in handle if line.strip()), "")
        if not first.startswith("## %s: " % match.group(1)):
            found.append("%s/%s: must start with \"## %s: <title>\", not %r"
                         % (ENTRIES, name, match.group(1), first))
    return found


def frozen_digest(repo):
    with open(os.path.join(repo, FROZEN), "rb") as handle:
        return hashlib.sha256(handle.read()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--list", action="store_true", help="list entry files in date order")
    parser.add_argument("--check", action="store_true", help="validate entry file names and headings")
    parser.add_argument("--check-frozen", action="store_true", help="fail if migration/LEDGER.md changed")
    args = parser.parse_args()
    repo = default_repo()
    if args.list:
        for name in entry_files(repo):
            print(os.path.join(ENTRIES, name))
        return 0
    if args.check_frozen:
        if frozen_digest(repo) != FROZEN_SHA256:
            print("%s is frozen: move your entry to a new file under %s/ (see %s/README.md)"
                  % (FROZEN, ENTRIES, ENTRIES))
            return 1
        print("%s unchanged" % FROZEN)
        return 0
    if args.check:
        found = problems(repo)
        for problem in found:
            print(problem)
        print("%d ledger entries, %d problems" % (len(entry_files(repo)), len(found)))
        return 1 if found else 0
    sys.stdout.write(read_text(repo))
    return 0


if __name__ == "__main__":
    sys.exit(main())
