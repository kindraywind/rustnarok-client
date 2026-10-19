#!/usr/bin/env python3
"""QA driver for rustnarok-client over its control socket (src/control.rs).

Accounts come from RUSTNAROK_CLIENT_ACCOUNTS, else this checkout's docs/personal/accounts.txt (a lane's), else
~/.config/rustnarok/accounts.txt; the server from
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
# A checkout's own accounts (a lane's, from lane.sh) come before the shared default, so lanes don't log each other out.
_LOCAL_ACCOUNTS = os.path.join(REPO, "docs", "personal", "accounts.txt")
ACCOUNTS = os.environ.get("RUSTNAROK_CLIENT_ACCOUNTS") or (
    _LOCAL_ACCOUNTS if os.path.exists(_LOCAL_ACCOUNTS) else os.path.expanduser("~/.config/rustnarok/accounts.txt")
)


CLIENTS = {"first": ("qa.sock", "client.log"), "second": ("qa2.sock", "client2.log")}


def use(client="first"):
    """Points every call that follows at one client: "first" (qa.sock) or "second" (qa2.sock), for two-player tests."""
    global SOCK, LOG
    sock, log = CLIENTS[client]
    SOCK, LOG = os.path.join(REPO, sock), os.path.join(REPO, "docs", "personal", log)


def account(n):
    fields = {}
    with open(ACCOUNTS) as f:
        for line in f:
            key, _, value = line.partition(":")
            fields[key.strip()] = value.strip()
    return fields["username" + n], fields["password" + n]


def connect(timeout):
    if os.name != "nt":
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(timeout)
        s.connect(SOCK)
        return s
    # Windows: the control "socket" is a file naming a localhost port and the token to open with (src/control.rs).
    with open(SOCK) as f:
        address, token = f.read().split()
    host, port = address.rsplit(":", 1)
    s = socket.create_connection((host, int(port)), timeout)
    s.sendall((token + "\n").encode())
    return s


def send(command, timeout=60):
    """The client's answer; a client that doesn't answer in time fails the command, not the run."""
    try:
        with connect(timeout) as s:
            s.sendall((json.dumps(command) + "\n").encode())
            data = b""
            while not data.endswith(b"\n"):
                chunk = s.recv(1 << 20)
                if not chunk:
                    break
                data += chunk
    except (socket.timeout, OSError) as error:
        return {"ok": False, "error": f"no answer: {error}"}
    return json.loads(data)


def launch(n="", slot="0", binary=None, env_extra=None, quiet=False):
    user, password = account(n)
    env = dict(os.environ)
    env.update(env_extra or {})
    # The player's own UserKeys.lua in the settings folder would move keys the scenarios press. A scenario's env
    # (or the shell) can still name one.
    env.setdefault("RUSTNAROK_CLIENT_USERKEYS", os.devnull)
    env.update(
        RUSTNAROK_CLIENT_CONTROL=SOCK,
        RUSTNAROK_CLIENT_SERVER=os.environ.get("RUSTNAROK_CLIENT_SERVER", "127.0.0.1:6900"),
        RUSTNAROK_CLIENT_LOGIN=f"{user}:{password}",
    )
    # None stays on the char-server list (a server_select scenario). Anything else autostarts.
    if slot is None:
        env.pop("RUSTNAROK_CLIENT_AUTOSTART", None)
    else:
        env["RUSTNAROK_CLIENT_AUTOSTART"] = slot
    if os.path.exists(SOCK):
        os.remove(SOCK)
    os.makedirs(os.path.dirname(LOG), exist_ok=True)
    log = open(LOG, "w")
    exe = "rustnarok-client.exe" if os.name == "nt" else "rustnarok-client"
    subprocess.Popen([binary or os.path.join(REPO, "target", "release", exe)], cwd=REPO, env=env, stdout=log, stderr=log)
    for _ in range(120):
        if os.path.exists(SOCK):
            try:
                state = send({"cmd": "state"})
                if not quiet:
                    print(json.dumps(state, ensure_ascii=False)[:300])
                return state
            except (OSError, ValueError):  # not up yet, or (Windows) the port file half written
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
