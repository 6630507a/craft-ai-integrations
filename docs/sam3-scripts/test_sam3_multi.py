"""Drive the installed PhotoCraft test build through the multi-prompt SAM 3 flow over the control channel."""
import json, socket, sys, time
from pathlib import Path

SCRATCH = Path.home() / "AppData" / "Local" / "hermes" / "cache" / "scratch"
TOKEN = (SCRATCH / "photocraft-dev.token").read_text().strip()
sock = socket.create_connection(("127.0.0.1", 7878), timeout=20)
_ids = iter(range(1, 10000))
buf = b""


def rpc(method, params=None):
    global buf
    payload = {"id": next(_ids), "method": method, "params": params or {}}
    sock.sendall((json.dumps(payload) + "\n").encode())
    while b"\n" not in buf:
        chunk = sock.recv(1 << 20)
        if not chunk:
            raise SystemExit("connection closed")
        buf += chunk
    line, buf = buf.split(b"\n", 1)
    reply = json.loads(line)
    if not reply.get("ok"):
        raise SystemExit(f"rpc error {method}: {reply}")
    return reply.get("result")


rpc("auth", {"token": TOKEN})


def status():
    r = rpc("ui.inspect")
    return str(r.get("status", "")), bool(r.get("statusError", False))


def wait(pred, secs=120, label=""):
    end = time.time() + secs
    last = None
    while time.time() < end:
        s, err = status()
        last = s
        if err and s:
            print("  error status:", s)
        if pred(s):
            return s
        time.sleep(0.5)
    sys.exit(f"TIMEOUT {label}: last status {last!r}")


def selection_probe():
    """Return which of the probe points are inside the selection, via select.* mask sampling on the engine."""
    r = rpc("ui.inspect")
    return r.get("selection") or r.get("documentSelection")


from PIL import Image, ImageDraw

# A "person": head disc over a body ellipse, plus a separate square object that must stay out.
img = Image.new("RGB", (512, 512), (235, 235, 230))
d = ImageDraw.Draw(img)
d.ellipse((216, 60, 296, 140), fill=(224, 172, 140))      # head
d.ellipse((176, 150, 336, 400), fill=(40, 70, 140))       # torso
d.rectangle((400, 380, 480, 460), fill=(190, 40, 40))     # other object
p = SCRATCH / "sam3-person.png"
img.save(p)

print("open:", rpc("app.open", {"path": p.name}).get("name"))
rpc("ui.set", {"tool": "objectSelection"})
wait(lambda s: s.startswith("Click"), label="ready")
print("ready")

# 1) Two positive points on different parts of the same object, then Enter.
rpc("ui.pointer", {"events": [{"kind": "down", "x": 256, "y": 100}, {"kind": "up", "x": 256, "y": 100}]})
time.sleep(2)
rpc("ui.pointer", {"events": [{"kind": "down", "x": 256, "y": 280}, {"kind": "up", "x": 256, "y": 280}]})
time.sleep(2)
print("after two clicks:", status())
rpc("ui.key", {"key": "Enter"})
wait(lambda s: "selected" in s.lower() or "nothing" in s.lower(), label="commit")
print("after Enter:", status())
shot = rpc("ui.screenshot", {"path": "sam3-multi-1.png"})
print("shot1:", shot)

cmds = {c["id"]: c for c in rpc("engine.commands")}
print("deselect enabled:", cmds.get("select.deselect", {}).get("enabled"))
