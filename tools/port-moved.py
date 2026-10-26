"""For a rebase, merge or cherry-pick stopped on a conflict in a file whose code has since moved into other files (such as
src/main.rs's methods into src/input.rs, src/events.rs, src/draw.rs and src/game.rs): takes the other side's version of
that file, then applies this branch's own hunks of it wherever their code now is, in that file or in the files it moved
to. A function that moved into another module got `pub(crate)`, and a module's body that became a file of its own (a
`mod tests { }` in tests.rs) lost an indent: the hunks are retried with those.

Usage, while git is stopped on the conflict:
    python3 -I tools/port-moved.py src/main.rs
Then look over the result (`git diff`, build), `git add` the files it names and continue. Hunks it couldn't place are
printed; apply those by hand.
"""
import os
import re
import subprocess
import sys


def git(*args, stdin=None, check=True):
    result = subprocess.run(["git", *args], capture_output=True, text=True, input=stdin)
    if check and result.returncode:
        sys.exit(f"git {' '.join(args)}: {result.stderr.strip()}")
    return result


def exists(ref):
    return git("rev-parse", "-q", "--verify", ref, check=False).returncode == 0


if len(sys.argv) != 2:
    sys.exit(__doc__)
path = git("rev-parse", "--show-prefix").stdout.strip() + sys.argv[1]
os.chdir(git("rev-parse", "--show-toplevel").stdout.strip())
# (the other side, the base of this branch's change, this branch's change)
if exists("REBASE_HEAD"):
    theirs, base, ours = "HEAD", "REBASE_HEAD^", "REBASE_HEAD"
elif exists("CHERRY_PICK_HEAD"):
    theirs, base, ours = "HEAD", "CHERRY_PICK_HEAD^", "CHERRY_PICK_HEAD"
elif exists("MERGE_HEAD"):
    theirs, ours = "MERGE_HEAD", "HEAD"
    base = git("merge-base", "HEAD", "MERGE_HEAD").stdout.strip()
else:
    sys.exit("No rebase, merge or cherry-pick is stopped here.")

hunks = []
for line in git("diff", "--no-color", "--no-ext-diff", base, ours, "--", path).stdout.splitlines(keepends=True):
    if line.startswith("@@"):
        hunks.append([line])
    elif hunks:
        hunks[-1].append(line)
if not hunks:
    sys.exit(f"This branch's change doesn't touch {path}.")

# Where the code may be now: the file itself, then the files the other side added or changed since this branch's base.
suffix = path[path.rfind(".") :]
conflicted = set(git("diff", "--name-only", "--diff-filter=U").stdout.split())
# --no-renames: a moved file (src/chat.rs → src/ui/chat.rs) counts as added there.
changed = git("diff", "--name-only", "--no-renames", "--diff-filter=AM", base, theirs).stdout.split()
places = [path] + [f for f in changed if f.endswith(suffix) and f != path and f not in conflicted]

# The other side moved the whole file away: drop it here, and the hunks go where its code is now.
gone = git("cat-file", "-e", f"{theirs}:{path}", check=False).returncode != 0
if gone:
    git("rm", "-q", "-f", "--", path)
    places.remove(path)
else:
    git("checkout", theirs, "--", path)


def made_public(hunk):
    return [hunk[0]] + [re.sub(r"^([ +-])((?:    )?)fn ", r"\1\2pub(crate) fn ", line) for line in hunk[1:]]


def dedented(hunk):
    return [hunk[0]] + [line[0] + line[5:] if line[1:5] == "    " else line for line in hunk[1:]]


def variants(hunk):
    return (hunk, made_public(hunk), dedented(hunk), made_public(dedented(hunk)))


def apply(place, lines, *flags):
    patch = f"--- a/{place}\n+++ b/{place}\n" + "".join(lines)
    if git("apply", "--check", "--recount", *flags, "-", stdin=patch, check=False).returncode:
        return False
    git("apply", "--recount", *flags, "-", stdin=patch)
    return True


def only_home(lines):
    """The one place holding the hunk's first removed line, if exactly one line anywhere matches it."""
    removed = next((line[1:].rstrip("\n") for line in lines[1:] if line.startswith("-")), None)
    if removed is None:
        return None
    homes = [(place, open(place).read().split("\n").count(removed)) for place in places]
    found = [place for place, count in homes if count]
    return found[0] if len(found) == 1 and dict(homes)[found[0]] == 1 else None


touched, left = set(), []
for hunk in hunks:
    placed = None
    for lines in variants(hunk):
        placed = next((place for place in places if apply(place, lines)), None)
        if placed:
            break
    # A hunk at the edge of what moved has context from a neighbour that stayed or went elsewhere: with less context,
    # where a removed line pins it to one spot.
    for lines in variants(hunk):
        home = None if placed else only_home(lines)
        if home and apply(home, lines, "-C0"):
            placed = home
            break
    if placed:
        touched.add(placed)
    else:
        left.append(hunk)

print(f"{path}: the other side's version, with {len(hunks) - len(left)} of {len(hunks)} hunks of this branch's change applied.")
if touched:
    print("Changed:", " ".join(sorted(touched)))
for hunk in left:
    print("\nNot placed (apply it by hand):\n" + "".join(hunk), end="")
print(f"\nThen: git add {' '.join(sorted(touched | (set() if gone else {path})))}")
sys.exit(1 if left else 0)
