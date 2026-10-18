#!/usr/bin/env bash
# Lanes: one git worktree per ticket, so several tickets can be worked at once (each with its own build, clients and
# test accounts) against one rathena.
#
#   lane.sh new <name> [--branch <b>] [--accounts <file>] [--reset <file>]
#       a worktree at ../rustnarok-client-lanes/<name> (RUSTNAROK_LANES to put them elsewhere) on a new branch (default
#       <name>) off a fresh origin/main; the game data and your local skills linked in from this checkout. --accounts and
#       --reset become the lane's docs/personal/accounts.txt and docs/personal/scenario-reset.sh, which drive.py and
#       visual.py use when the RUSTNAROK_CLIENT_ACCOUNTS / RUSTNAROK_CLIENT_SCENARIO_RESET variables aren't set.
#   lane.sh list            each lane: branch, merged into origin/main or not, clients running
#   lane.sh rm <name>       quits the lane's clients, removes it (and its baseline worktree), deletes its branch when
#                           merged; refuses a lane with uncommitted changes unless --force
#   lane.sh prune           rm for every lane whose branch is merged and has no client running
set -euo pipefail

# The main checkout: the parent of the repository's shared .git.
MAIN=$(cd "$(git -C "$(dirname "$0")" rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
LANES=${RUSTNAROK_LANES:-$(dirname "$MAIN")/rustnarok-client-lanes}
# Gitignored things a checkout needs that git doesn't bring: the game data and each developer's own skills.
LINKED="data.grf data System SystemEN BGM Ragnarok.app"

die() { echo "lane: $*" >&2; exit 1; }

clients() { pgrep -f "$1/target/" 2>/dev/null || true; }

merged() { git -C "$MAIN" merge-base --is-ancestor "$1" origin/main 2>/dev/null; }

new() {
    local name=$1 branch=$1 accounts="" reset=""
    shift
    while [ $# -gt 0 ]; do
        case $1 in
            --branch) branch=$2; shift 2 ;;
            --accounts) accounts=$2; shift 2 ;;
            --reset) reset=$2; shift 2 ;;
            *) die "unknown option $1" ;;
        esac
    done
    local lane="$LANES/$name"
    [ -e "$lane" ] && die "$lane exists"
    git -C "$MAIN" fetch -q origin main
    mkdir -p "$LANES"
    git -C "$MAIN" worktree add -q -b "$branch" "$lane" origin/main
    # The main checkout's build, cloned copy-on-write (instant, no disk until it changes): the lane's first build then
    # compiles only this crate, not every dependency. ponytail: macOS only; elsewhere `cp --reflink` would do it.
    [ "$(uname)" = Darwin ] && [ -d "$MAIN/target" ] && cp -c -R "$MAIN/target" "$lane/target" 2>/dev/null || true
    for item in $LINKED; do
        if [ -L "$MAIN/$item" ]; then
            ln -s "$(readlink -f "$MAIN/$item")" "$lane/$item"
        elif [ -e "$MAIN/$item" ]; then
            ln -s "$MAIN/$item" "$lane/$item"
        fi
    done
    for skill in "$MAIN"/.claude/skills/local-*; do
        [ -e "$skill" ] && ln -s "$skill" "$lane/.claude/skills/$(basename "$skill")"
    done
    mkdir -p "$lane/docs/personal"
    [ -n "$accounts" ] && cp "$accounts" "$lane/docs/personal/accounts.txt" && chmod 600 "$lane/docs/personal/accounts.txt"
    [ -n "$reset" ] && cp "$reset" "$lane/docs/personal/scenario-reset.sh"
    echo "$lane  (branch $branch)"
    [ -z "$accounts" ] && echo "  no --accounts: its clients log in with the default accounts, which another lane may be using"
    return 0
}

list() {
    # One line per worktree: its path and branch ("-" when detached).
    git -C "$MAIN" worktree list --porcelain | awk '/^worktree /{path=$2; ref="-"} /^branch /{ref=$2} /^$/{print path, ref}' | while read -r path ref; do
        case $path in "$LANES"/*) ;; *) continue ;; esac
        local branch=${ref#refs/heads/} state=open running
        [ "$branch" != "-" ] && merged "$branch" && state=merged
        running=$(clients "$path" | wc -l | tr -d ' ')
        echo "$(basename "$path")  $branch  $state  clients:$running"
    done
}

rm_lane() {
    local name=$1 force=${2:-}
    local lane="$LANES/$name"
    [ -d "$lane" ] || die "no lane $name"
    # The lane's branch, even when its HEAD was detached since.
    local branch
    branch=$(git -C "$MAIN" worktree list --porcelain | awk -v lane="$lane" '/^worktree /{path=$2} /^branch /{if (path == lane) print substr($2, 12)}')
    [ -n "$branch" ] || branch=$name
    if [ "$force" != "--force" ] && [ -n "$(git -C "$lane" status --porcelain)" ]; then
        die "$name has uncommitted changes (--force to drop them)"
    fi
    # A PR merged a moment ago counts: see origin/main as it is now.
    git -C "$MAIN" fetch -q origin main || true
    local pids
    pids=$(clients "$lane")
    [ -n "$pids" ] && kill $pids 2>/dev/null || true
    # visual.py's baseline worktree lives inside the lane; it's registered with git too.
    [ -d "$lane/docs/personal/baseline-tree" ] && git -C "$MAIN" worktree remove --force "$lane/docs/personal/baseline-tree" 2>/dev/null || true
    git -C "$MAIN" worktree remove --force "$lane"
    git -C "$MAIN" worktree prune
    if ! git -C "$MAIN" rev-parse -q --verify "refs/heads/$branch" >/dev/null; then
        echo "removed $name"
    elif merged "$branch"; then
        git -C "$MAIN" branch -q -D "$branch" && echo "removed $name, deleted merged branch $branch"
    else
        echo "removed $name, kept unmerged branch $branch"
    fi
}

prune() {
    git -C "$MAIN" fetch -q origin main
    list | while read -r name _ state running; do
        [ "$state" = merged ] && [ "$running" = "clients:0" ] && rm_lane "$name" || true
    done
}

case ${1:-} in
    new) shift; [ $# -ge 1 ] || die "new <name>"; new "$@" ;;
    list) list ;;
    rm) shift; [ $# -ge 1 ] || die "rm <name>"; rm_lane "$@" ;;
    prune) prune ;;
    *) sed -n '2,19p' "$0"; exit 1 ;;
esac
