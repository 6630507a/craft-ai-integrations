import io, json, time, urllib.request, urllib.parse, base64
BASE = "http://127.0.0.1:8000"
NODE = "3"


def post(path, payload, timeout=240):
    req = urllib.request.Request(BASE + path, data=json.dumps(payload).encode(), headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.load(r)


def get(path):
    with urllib.request.urlopen(BASE + path, timeout=60) as r:
        return r.read()


def timed(label, fn):
    t = time.time()
    out = fn()
    print(f"{label:38s} {time.time() - t:6.2f}s")
    return out


def upload(name, data):
    b = "bnd123"
    body = (f"--{b}\r\nContent-Disposition: form-data; name=\"type\"\r\n\r\ninput\r\n--{b}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{name}\"\r\nContent-Type: image/png\r\n\r\n").encode() + data + f"\r\n--{b}--\r\n".encode()
    req = urllib.request.Request(BASE + "/upload/image", data=body, headers={"Content-Type": f"multipart/form-data; boundary={b}"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)["name"]


def run(graph):
    pid = post("/prompt", {"prompt": graph})["prompt_id"]
    while True:
        h = json.loads(get(f"/history/{pid}"))
        rec = h.get(pid)
        if rec and rec.get("status", {}).get("completed"):
            return rec
        if rec and rec.get("status", {}).get("status_str") == "error":
            raise SystemExit(rec["status"])
        time.sleep(0.15)


def graph(fn, prompts, mask):
    g = {"1": {"class_type": "LoadImage", "inputs": {"image": fn}},
         "2": {"class_type": "LoadSAM3Model", "inputs": {"precision": "auto", "compile": False}},
         NODE: {"class_type": "SAM3InteractiveCollector", "inputs": {"sam3_model_config": ["2", 0], "image": ["1", 0], "multi_prompts_store": json.dumps(prompts)}}}
    if mask:
        g["4"] = {"class_type": "MaskToImage", "inputs": {"mask": [NODE, 0]}}
        g["5"] = {"class_type": "SaveImage", "inputs": {"images": ["4", 0], "filename_prefix": "timing"}}
    return g


import sys
from PIL import Image
S = int(sys.argv[1]) if len(sys.argv) > 1 else 512
_im = Image.open(r"C:\Users\jerem\AppData\Local\hermes\cache\scratch\astronaut.png").resize((S, S))
_b = io.BytesIO(); _im.save(_b, "PNG"); data = _b.getvalue()
k = S / 512
P = lambda x, y: (int(x * k), int(y * k))
timed("object_info (every prepare + ground)", lambda: get("/object_info"))
fn = timed("upload", lambda: upload("timing-astro-%d.png" % S + "", data))
nonce = 1234567 + S
timed("prepare graph (embedding)", lambda: run(graph(fn, [{"session": nonce}], False)))
timed("prepare graph again (cached?)", lambda: run(graph(fn, [{"session": nonce}], False)))
for pts in ([P(256,150)], [P(256,150), P(256,400)], [P(256,150), P(256,400), P(100,300)]):
    prompt = {"positive_points": [{"x": x, "y": y} for x, y in pts], "negative_points": [], "positive_boxes": []}
    for i in range(3):
        r = timed(f"hover preview, {len(pts)} pts (#{i+1})", lambda: post("/sam3/interactive_segment_one", {"node_id": NODE, "prompt_index": 0, "prompt_name": "Object selection", "prompt": prompt}))
    print("   num_masks", r.get("num_masks"), "overlay KB", len(r.get("overlay", "")) // 1365)
timed("commit graph", lambda: run(graph(fn, [{"positive_points": [{"x": 256, "y": 150}], "session": nonce}], True)))
