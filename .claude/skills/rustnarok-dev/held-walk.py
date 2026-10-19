#!/usr/bin/env python3
"""Held walk: holds the left button down on the ground and samples where the player is drawn, to catch jumps.

  held-walk.py [x y] [seconds] [limit]

With a client up on qa.sock (drive.py launch), presses at x, y (default 812 384, east of the player) and keeps the
button down for `seconds` (4) while the client asks the server again every few hundred ms, polling `state` for
game.player.position and drawn_ms (the frame it was drawn on) every 10 ms; then lets go. Prints the frames' spacing
and the largest move from one frame seen to the next, and exits 1 when a move over `limit` cells (0.3) came in a frame
step of 20 ms or less: a jump, not a slow frame.
"""
import math
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import drive  # noqa: E402


def main(args):
    x, y = (int(args[0]), int(args[1])) if len(args) >= 2 else (812, 384)
    seconds = float(args[2]) if len(args) >= 3 else 4.0
    limit = float(args[3]) if len(args) >= 4 else 0.3
    drive.send({"cmd": "drag", "from": [x, y], "to": [x, y], "held": True})
    samples, polls = [], 0
    start = time.monotonic()
    try:
        while time.monotonic() < start + seconds:
            polls += 1
            player = drive.send({"cmd": "state"})["game"]["player"]
            # Each frame once.
            if not samples or player["drawn_ms"] != samples[-1][0]:
                samples.append((player["drawn_ms"], player["position"]))
            # One sample every 10 ms.
            time.sleep(max(0.0, start + polls * 0.01 - time.monotonic()))
    finally:
        drive.send({"cmd": "release"})
    moves = [(math.dist(a, b), t1 - t0, b) for (t0, a), (t1, b) in zip(samples, samples[1:])]
    steps = sorted(ms for _, ms, _ in moves)
    largest = max(moves)
    print(f"{len(samples)} frames seen, {steps[len(steps) // 2]} ms apart (median, longest {steps[-1]})")
    print(f"walked {math.dist(samples[0][1], samples[-1][1]):.2f} cells; largest move {largest[0]:.3f} cells in {largest[1]} ms, to {largest[2]}")
    jumps = [move for move in moves if move[0] > limit and move[1] <= 20]
    for distance, ms, at in jumps:
        print(f"  jump: {distance:.3f} cells in {ms} ms, to {at}")
    return 1 if jumps else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
