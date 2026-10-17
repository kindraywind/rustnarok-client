#!/usr/bin/env python3
"""QA driver for rustnarok-client over its control socket (src/control.rs).

Accounts come from RUSTNAROK_CLIENT_ACCOUNTS (default ~/.config/rustnarok/accounts.txt); the server from
RUSTNAROK_CLIENT_SERVER (default 127.0.0.1:6900). Screenshots and the client log go under docs/personal/, which
is gitignored: game images never go in the repository.

  drive.py launch [account] [slot]   start the client logged in (account "" or "2"; slot 0), in the background
  drive.py send '<json>'             one command, prints the reply (a screenshot's png is left out)
  drive.py shot <name> [x y w h]     screenshot (optionally cropped) to docs/personal/shots/<name>.png
  drive.py state                     the game state, pretty-printed
  drive.py quit
"""
import base64
import json
import os
import socket
import subprocess
import sys
import time

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
SOCK = os.path.join(REPO, "qa.sock")
TODAY = time.strftime("%Y-%m-%d")
SHOTS = os.path.join(REPO, "docs", "personal", "shots", TODAY)
LOG = os.path.join(REPO, "docs", "personal", "client.log")
ACCOUNTS = os.environ.get("RUSTNAROK_CLIENT_ACCOUNTS", os.path.expanduser("~/.config/rustnarok/accounts.txt"))


def account(n):
    fields = {}
    with open(ACCOUNTS) as f:
        for line in f:
            key, _, value = line.partition(":")
            fields[key.strip()] = value.strip()
    return fields["username" + n], fields["password" + n]


def send(command, timeout=60):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(timeout)
        s.connect(SOCK)
        s.sendall((json.dumps(command) + "\n").encode())
        data = b""
        while not data.endswith(b"\n"):
            chunk = s.recv(1 << 20)
            if not chunk:
                break
            data += chunk
    return json.loads(data)


def launch(n="", slot="0", binary=None, env_extra=None, quiet=False):
    user, password = account(n)
    env = dict(os.environ)
    env.update(env_extra or {})
    env.update(
        RUSTNAROK_CLIENT_CONTROL=SOCK,
        RUSTNAROK_CLIENT_SERVER=os.environ.get("RUSTNAROK_CLIENT_SERVER", "127.0.0.1:6900"),
        RUSTNAROK_CLIENT_LOGIN=f"{user}:{password}",
        RUSTNAROK_CLIENT_AUTOSTART=slot,
    )
    if os.path.exists(SOCK):
        os.remove(SOCK)
    os.makedirs(os.path.dirname(LOG), exist_ok=True)
    log = open(LOG, "w")
    subprocess.Popen([binary or os.path.join(REPO, "target", "release", "rustnarok-client")], cwd=REPO, env=env, stdout=log, stderr=log)
    for _ in range(120):
        if os.path.exists(SOCK):
            try:
                state = send({"cmd": "state"})
                if not quiet:
                    print(json.dumps(state, ensure_ascii=False)[:300])
                return state
            except OSError:
                pass
        time.sleep(0.5)
    sys.exit("no control socket; see " + LOG)


def quit_quietly():
    try:
        send({"cmd": "quit"}, timeout=5)
    except (OSError, ValueError):
        pass


def shot(name, crop=None, out=None):
    command = {"cmd": "screenshot"}
    if crop:
        command["crop"] = [int(v) for v in crop]
    reply = send(command)
    if not reply.get("ok"):
        sys.exit(json.dumps(reply))
    # Manual shots: shots/<date>/<HHMMSS>_<name>.png; a caller's own folder (a visual run) keeps the name.
    if out is None:
        out, name = SHOTS, time.strftime("%H%M%S_") + name
    os.makedirs(out, exist_ok=True)
    path = os.path.join(out, name + ".png")
    with open(path, "wb") as f:
        f.write(base64.b64decode(reply["png"]))
    return path


if __name__ == "__main__":
    what, args = sys.argv[1], sys.argv[2:]
    if what == "launch":
        launch(*args)
    elif what == "send":
        reply = send(json.loads(args[0]))
        reply.pop("png", None)
        print(json.dumps(reply, ensure_ascii=False))
    elif what == "shot":
        print(shot(args[0], args[1:5] or None))
    elif what == "state":
        print(json.dumps(send({"cmd": "state"}), ensure_ascii=False, indent=1))
    elif what == "quit":
        print(send({"cmd": "quit"}))
