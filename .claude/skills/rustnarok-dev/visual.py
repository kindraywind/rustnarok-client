#!/usr/bin/env python3
"""Visual verification for rustnarok-client: scripted scenarios, screenshots, and before/after diffs.

  visual.py run <scenario.json>... [--bin <client>] [--out <dir>]   play scenarios, save each shot (+ its state)
  visual.py baseline <scenario.json>...                             the same, on origin/main (built in a worktree)
  visual.py diff <before dir> <after dir> [--out <dir>]             per-shot change and before|after|diff sheets
  visual.py check <scenario.json>...                                baseline, this tree, then diff: the PR's pictures

A scenario is JSON: {"account": "", "slot": "0", "env": {...}, "steps": [...]} with steps such as
  {"say": "@warp prontera 156 180"}  {"settle": 1500}  {"wait": 1500}  {"key": "Alt+E"}  {"type": "hello"}
  {"click": [x, y], "button": "right", "count": 2}  {"drag": [[x, y], [x2, y2]]}  {"move": [x, y]}
  {"shot": "name", "crop": [x, y, w, h]}  {"cmd": {...raw control command...}}
`settle` waits until the map has loaded again, then the given ms (GM commands sent while it loads are lost).
Each scenario starts a fresh client; the clock is paused for every shot so frames repeat.
Everything lands under docs/personal/visual/<date>/<HHMMSS>/ (gitignored): game images never go in the repository.
"""
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import drive  # noqa: E402

REPO = drive.REPO
# One folder per run: visual/<date>/<HHMMSS>/{main,branch,diff}.
VISUAL = os.path.join(REPO, "docs", "personal", "visual", drive.TODAY, time.strftime("%H%M%S"))
WORKTREE = os.path.join(REPO, "docs", "personal", "baseline-tree")


def wait_for_game(timeout=60):
    """Until the player is on a map and loading is done; a refused login ("already online") is retried."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        state = drive.send({"cmd": "state"})
        if state.get("scene") == "game" and not state.get("loading"):
            return state
        time.sleep(0.5)
    raise RuntimeError(f"not in the game after {timeout}s: {json.dumps(state)[:300]}")


def launch(binary, scenario):
    env_extra = scenario.get("env", {})
    for attempt in range(6):
        drive.launch(scenario.get("account", ""), str(scenario.get("slot", "0")), binary=binary, env_extra=env_extra, quiet=True)
        try:
            return wait_for_game()
        except RuntimeError:
            drive.quit_quietly()
            time.sleep(10)  # the server still holds the last session
    raise RuntimeError("could not get into the game")


def step(action, shots_dir, name):
    if "say" in action:
        return drive.send({"cmd": "say", "text": action["say"]})
    if "wait" in action:
        return time.sleep(action["wait"] / 1000)
    if "settle" in action:
        # After a warp or anything that reloads the map: back in the game, then a moment for the server.
        time.sleep(0.5)
        wait_for_game()
        return time.sleep(action["settle"] / 1000)
    if "key" in action:
        return drive.send({"cmd": "key", "key": action["key"]})
    if "type" in action:
        return drive.send({"cmd": "type", "text": action["type"]})
    if "move" in action:
        x, y = action["move"]
        return drive.send({"cmd": "move", "x": x, "y": y})
    if "click" in action:
        x, y = action["click"]
        command = {"cmd": "click", "x": x, "y": y}
        for key in ("button", "count", "mods"):
            if key in action:
                command[key] = action[key]
        return drive.send(command)
    if "drag" in action:
        start, end = action["drag"]
        return drive.send({"cmd": "drag", "from": start, "to": end})
    if "cmd" in action:
        return drive.send(action["cmd"])
    if "shot" in action:
        drive.send({"cmd": "clock", "action": "pause"})
        path = drive.shot(action["shot"], action.get("crop"), shots_dir)
        state = drive.send({"cmd": "state"})
        with open(os.path.splitext(path)[0] + ".state.json", "w") as f:
            json.dump(state, f, ensure_ascii=False, indent=1)
        drive.send({"cmd": "clock", "action": "resume"})
        print(f"  {name}: {os.path.relpath(path, REPO)}")
        return None
    raise ValueError(f"unknown step {action}")


def run(scenarios, binary, out):
    os.makedirs(out, exist_ok=True)
    for path in scenarios:
        scenario = json.load(open(path))
        name = os.path.splitext(os.path.basename(path))[0]
        print(f"{name}:")
        launch(binary, scenario)
        try:
            for action in scenario["steps"]:
                step(action, out, name)
        finally:
            drive.quit_quietly()
            time.sleep(2)
    return out


def baseline(scenarios):
    """Builds origin/main in a worktree and plays the scenarios with it, against this repo's data."""
    subprocess.run(["git", "fetch", "-q", "origin"], cwd=REPO, check=True)
    if not os.path.isdir(WORKTREE):
        subprocess.run(["git", "worktree", "add", "-q", "--detach", WORKTREE, "origin/main"], cwd=REPO, check=True)
    else:
        subprocess.run(["git", "checkout", "-q", "--detach", "origin/main"], cwd=WORKTREE, check=True)
    head = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=WORKTREE, capture_output=True, text=True).stdout.strip()
    subprocess.run(["cargo", "build", "--release", "-q"], cwd=WORKTREE, check=True)
    print(f"baseline: origin/main {head}")
    return run(scenarios, os.path.join(WORKTREE, "target", "release", "rustnarok-client"), os.path.join(VISUAL, "main"))


def diff(before, after, out):
    from PIL import Image, ImageChops, ImageDraw

    os.makedirs(out, exist_ok=True)
    rows = []
    for name in sorted(f for f in os.listdir(after) if f.endswith(".png")):
        new = Image.open(os.path.join(after, name)).convert("RGB")
        old_path = os.path.join(before, name)
        if not os.path.exists(old_path):
            rows.append((name, "new", None))
            continue
        old = Image.open(old_path).convert("RGB")
        if old.size != new.size:
            rows.append((name, f"size {old.size} -> {new.size}", None))
            continue
        delta = ImageChops.difference(old, new).convert("L").point(lambda v: 255 if v > 24 else 0)
        changed = delta.histogram()[255] / (delta.width * delta.height)
        # The diff: the new frame greyed, changed pixels in red.
        marked = Image.composite(Image.new("RGB", new.size, (255, 0, 0)), new.convert("L").convert("RGB"), delta)
        width, height = new.size
        sheet = Image.new("RGB", (width * 3 + 20, height + 24), (32, 32, 32))
        draw = ImageDraw.Draw(sheet)
        for column, (label, image) in enumerate((("before (main)", old), ("after (this branch)", new), (f"diff {changed:.2%}", marked))):
            sheet.paste(image, (column * (width + 10), 24))
            draw.text((column * (width + 10) + 4, 5), label, fill=(255, 255, 255))
        sheet_path = os.path.join(out, name.replace(".png", ".sheet.png"))
        sheet.save(sheet_path)
        rows.append((name, f"{changed:.2%}", sheet_path))
    print("| shot | changed | sheet |\n|---|---|---|")
    for name, changed, sheet in rows:
        print(f"| {name} | {changed} | {os.path.relpath(sheet, REPO) if sheet else ''} |")
    return rows


if __name__ == "__main__":
    what, args = sys.argv[1], sys.argv[2:]
    option = lambda flag, default: args[args.index(flag) + 1] if flag in args else default  # noqa: E731
    files = [a for i, a in enumerate(args) if a.endswith(".json") and (i == 0 or args[i - 1] not in ("--bin", "--out"))]
    if what == "run":
        run(files, option("--bin", None), option("--out", os.path.join(VISUAL, "branch")))
    elif what == "baseline":
        baseline(files)
    elif what == "diff":
        diff(args[0], args[1], option("--out", os.path.join(VISUAL, "diff")))
    elif what == "check":
        baseline(files)
        subprocess.run(["cargo", "build", "--release", "-q"], cwd=REPO, check=True)
        run(files, None, os.path.join(VISUAL, "branch"))
        diff(os.path.join(VISUAL, "main"), os.path.join(VISUAL, "branch"), os.path.join(VISUAL, "diff"))
    else:
        sys.exit(__doc__)
