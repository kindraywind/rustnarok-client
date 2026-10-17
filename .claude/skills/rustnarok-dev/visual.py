#!/usr/bin/env python3
"""Visual verification for rustnarok-client: scripted scenarios, screenshots, and before/after diffs.

  visual.py run <scenario.json>... [--bin <client>] [--out <dir>]   play scenarios, save each shot (+ its state)
  visual.py baseline <scenario.json>...                             the same, on origin/main (built in a worktree)
  visual.py diff <before dir> <after dir> [--out <dir>]             per-shot change and before|after|diff sheets
  visual.py check <scenario.json>...                                baseline, this tree, then diff: the PR's pictures
  visual.py suite                                                   check every scenario in scenarios/, one report

A scenario is JSON: {"account": "", "slot": "0", "env": {...}, "steps": [...]} with steps such as
  {"say": "@warp prontera 156 180"}  {"settle": 1500}  {"wait": 1500}  {"key": "Alt+E"}  {"type": "hello"}
  {"click": [x, y], "button": "right", "count": 2}  {"drag": [[x, y], [x2, y2]]}  {"move": [x, y]}
  {"shot": "name", "crop": [x, y, w, h], "mask": [[x, y, w, h], ...], "threshold": 1.0}  {"cmd": {...}}
  {"wait_for": {"event": "ItemUsed", "ms": 5000}}  (or "packet": "0x01c8": seen since the previous command)
  {"expect": {"chat": "Item created.", "inventory_items": 5, "hp_at_least": 40, "map": "iz_int01.gat",
              "windows_open": ["Inventory"], "windows_closed": ["Skills"], "units_at_least": {"monster": 10},
              "ground_items_at_least": 1, "hotkeys": {"0": 501, "3": null}}}
A failed `expect` or `wait_for` fails the scenario (the run goes on, so its shots still come out); `run`, `check`
and `suite` exit non-zero when the tree under test fails one. `mask` leaves rectangles (water, portals) out of the
diff; `threshold` (percent, default 1) is how much change a shot takes before the report flags it.
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
# cargo for the builds, also when the shell hasn't loaded ~/.cargo/env.
os.environ["PATH"] = os.path.expanduser("~/.cargo/bin") + os.pathsep + os.environ.get("PATH", "")
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
    # Scenarios start from the same server state: RUSTNAROK_CLIENT_SCENARIO_RESET is a shell command (each developer's
    # own, for their server) run before every launch, e.g. clearing the test characters' saved hotkey bars.
    reset = os.environ.get("RUSTNAROK_CLIENT_SCENARIO_RESET")
    if reset:
        subprocess.run(reset, shell=True, check=True)
    env_extra = scenario.get("env", {})
    for attempt in range(6):
        drive.launch(scenario.get("account", ""), str(scenario.get("slot", "0")), binary=binary, env_extra=env_extra, quiet=True)
        try:
            return wait_for_game()
        except RuntimeError:
            drive.quit_quietly()
            time.sleep(10)  # the server still holds the last session
    raise RuntimeError("could not get into the game")


def expect(wanted, state):
    """What in `state` doesn't match `wanted`, as messages; empty when everything does."""
    game, failures = state.get("game", {}), []
    chat = game.get("chat", [])
    for key, value in wanted.items():
        if key == "chat" and not any(value in line for line in chat):
            failures.append(f"chat has no {value!r} (last: {chat[-3:]})")
        elif key == "no_chat" and any(value in line for line in chat):
            failures.append(f"chat has {value!r}")
        elif key == "inventory_items" and game.get("inventory_items") != value:
            failures.append(f"inventory_items {game.get('inventory_items')} != {value}")
        elif key == "hp_at_least" and game.get("player", {}).get("hp", 0) < value:
            failures.append(f"hp {game.get('player', {}).get('hp')} < {value}")
        elif key == "map" and game.get("map") != value:
            failures.append(f"map {game.get('map')} != {value}")
        elif key == "windows_open":
            failures += [f"window {w} not open" for w in value if w not in game.get("windows", [])]
        elif key == "windows_closed":
            failures += [f"window {w} open" for w in value if w in game.get("windows", [])]
        elif key == "ground_items_at_least" and len(game.get("ground_items", [])) < value:
            failures.append(f"{len(game.get('ground_items', []))} items on the ground < {value}")
        elif key == "hotkeys":
            # {"<slot>": <item or skill id>, or null for an empty slot}
            slots = game.get("hotkeys", [])
            for slot, want in value.items():
                have = slots[int(slot)] if int(slot) < len(slots) else None
                if (have or {}).get("id") != want if want is not None else have is not None:
                    failures.append(f"hotkey {slot} holds {have}, not {want}")
        elif key == "units_at_least":
            for kind, count in value.items():
                have = sum(1 for unit in game.get("units", []) if unit.get("kind") == kind)
                if have < count:
                    failures.append(f"{have} {kind} units < {count}")
        elif key not in ("chat", "no_chat", "inventory_items", "hp_at_least", "map", "windows_open", "windows_closed", "units_at_least", "ground_items_at_least", "hotkeys"):
            failures.append(f"unknown expectation {key}")
    return failures


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
    if "wait_for" in action:
        command = {"cmd": "wait", **action["wait_for"]}
        command.setdefault("ms", 5000)
        reply = drive.send(command, timeout=command["ms"] / 1000 + 10)
        return None if reply.get("ok") else [f"wait_for {action['wait_for']}: {reply.get('error')}"]
    if "expect" in action:
        return expect(action["expect"], drive.send({"cmd": "state"}))
    if "shot" in action:
        drive.send({"cmd": "clock", "action": "pause"})
        path = drive.shot(action["shot"], action.get("crop"), shots_dir)
        state = drive.send({"cmd": "state"})
        with open(os.path.splitext(path)[0] + ".state.json", "w") as f:
            json.dump(state, f, ensure_ascii=False, indent=1)
        # How the diff treats this shot: masked rectangles (in the shot's own pixels) and the change it may take.
        with open(os.path.splitext(path)[0] + ".shot.json", "w") as f:
            json.dump({"mask": action.get("mask", []), "threshold": action.get("threshold", 1.0), "crop": action.get("crop")}, f)
        drive.send({"cmd": "clock", "action": "resume"})
        print(f"  {name}: {os.path.relpath(path, REPO)}")
        return None
    raise ValueError(f"unknown step {action}")


def run(scenarios, binary, out):
    """Plays the scenarios; returns each one's failed expectations (empty: it passed)."""
    os.makedirs(out, exist_ok=True)
    results = {}
    for path in scenarios:
        scenario = json.load(open(path))
        name = os.path.splitext(os.path.basename(path))[0]
        print(f"{name}:")
        launch(binary, scenario)
        failures = []
        try:
            for number, action in enumerate(scenario["steps"], 1):
                failed = step(action, out, name)
                if isinstance(failed, list) and failed:
                    failures += [f"step {number}: {message}" for message in failed]
                    print(f"  {name}: FAIL step {number}: {'; '.join(failed)}")
        finally:
            drive.quit_quietly()
            time.sleep(2)
        results[name] = failures
        print(f"  {name}: {'PASS' if not failures else f'FAIL ({len(failures)})'}")
    with open(os.path.join(out, "results.json"), "w") as f:
        json.dump(results, f, ensure_ascii=False, indent=1)
    return results


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
        settings_path = os.path.join(after, name.replace(".png", ".shot.json"))
        settings = json.load(open(settings_path)) if os.path.exists(settings_path) else {}
        for x, y, w, h in settings.get("mask", []):
            delta.paste(0, (x, y, x + w, y + h))
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
        flag = " ⚠" if changed * 100 > settings.get("threshold", 1.0) else ""
        rows.append((name, f"{changed:.2%}{flag}", sheet_path))
    print("| shot | changed | sheet |\n|---|---|---|")
    for name, changed, sheet in rows:
        print(f"| {name} | {changed} | {os.path.relpath(sheet, REPO) if sheet else ''} |")
    return rows


def report(scenarios, main_results, branch_results, rows):
    """docs/personal/visual/<date>/<time>/report.md: each scenario's expectations on main and here, then the shots."""
    lines = [f"# Visual check {drive.TODAY} {os.path.basename(VISUAL)}", "", "| scenario | main | this tree |", "|---|---|---|"]
    for path in scenarios:
        name = os.path.splitext(os.path.basename(path))[0]
        verdict = lambda results: "—" if name not in results else ("PASS" if not results[name] else "FAIL: " + "; ".join(results[name]))  # noqa: E731
        lines.append(f"| {name} | {verdict(main_results)} | {verdict(branch_results)} |")
    lines += ["", "| shot | changed (⚠ over its threshold) | sheet |", "|---|---|---|"]
    lines += [f"| {name} | {changed} | {os.path.relpath(sheet, VISUAL) if sheet else ''} |" for name, changed, sheet in rows]
    path = os.path.join(VISUAL, "report.md")
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"report: {os.path.relpath(path, REPO)}")


def check(scenarios):
    """Main, then this tree, then the diff and the report; exits non-zero when this tree fails an expectation."""
    main_results = baseline(scenarios)
    subprocess.run(["cargo", "build", "--release", "-q"], cwd=REPO, check=True)
    branch_results = run(scenarios, None, os.path.join(VISUAL, "branch"))
    rows = diff(os.path.join(VISUAL, "main"), os.path.join(VISUAL, "branch"), os.path.join(VISUAL, "diff"))
    report(scenarios, main_results, branch_results, rows)
    return branch_results


if __name__ == "__main__":
    what, args = sys.argv[1], sys.argv[2:]
    option = lambda flag, default: args[args.index(flag) + 1] if flag in args else default  # noqa: E731
    files = [a for i, a in enumerate(args) if a.endswith(".json") and (i == 0 or args[i - 1] not in ("--bin", "--out"))]
    results = {}
    if what == "run":
        results = run(files, option("--bin", None), option("--out", os.path.join(VISUAL, "branch")))
    elif what == "baseline":
        results = baseline(files)
    elif what == "diff":
        diff(args[0], args[1], option("--out", os.path.join(VISUAL, "diff")))
    elif what == "check":
        results = check(files)
    elif what == "suite":
        folder = os.path.join(HERE, "scenarios")
        results = check(sorted(os.path.join(folder, f) for f in os.listdir(folder) if f.endswith(".json")))
    else:
        sys.exit(__doc__)
    failed = [name for name, failures in results.items() if failures]
    if failed:
        sys.exit(f"FAILED: {', '.join(failed)}")
