"""Measure what one stale SAM 3 hover-cache entry costs the ComfyUI process that holds it.
Creates a few extra entries under the throwaway ids craft_ai_measure_N (not the slot ids and not
the shared craft_ai_sam3 id) and reads process and GPU memory around each. The entries remain on
the server until ComfyUI restarts, which is exactly the cost being measured. Nothing is installed
or configured. Output passes through scrub().
"""
import json
import subprocess
import sys
import urllib.request
import uuid
from pathlib import Path

from scrub import scrub

URL = 'http://127.0.0.1:8000'
HERE = Path(__file__).resolve().parent
IMAGE = HERE / 'evidence/astronaut-source.png'


def call(path, body=None, headers=None):
    request = urllib.request.Request(URL + path, data=body, headers=headers or {})
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.loads(response.read() or b'null')


def upload(data):
    boundary = uuid.uuid4().hex
    name = f'craft-measure-{uuid.uuid4().hex[:8]}.png'
    body = (f'--{boundary}\r\nContent-Disposition: form-data; name="image"; filename="{name}"\r\n'
            f'Content-Type: image/png\r\n\r\n').encode() + data + f'\r\n--{boundary}--\r\n'.encode()
    reply = call('/upload/image', body, {'Content-Type': f'multipart/form-data; boundary={boundary}'})
    return reply['name']


def run(node_id, filename):
    graph = {
        '1': {'class_type': 'LoadImage', 'inputs': {'image': filename}},
        '2': {'class_type': 'LoadSAM3Model', 'inputs': {'precision': 'auto', 'compile': False}},
        node_id: {'class_type': 'SAM3InteractiveCollector', 'inputs': {
            'sam3_model_config': ['2', 0], 'image': ['1', 0],
            'multi_prompts_store': json.dumps([{'session': uuid.uuid4().int % 10**12}])}},
    }
    prompt_id = call('/prompt', json.dumps({'prompt': graph}).encode(),
                     {'Content-Type': 'application/json'})['prompt_id']
    import time
    for _ in range(600):
        history = call(f'/history/{prompt_id}')
        record = history.get(prompt_id)
        if record and record.get('status', {}).get('completed'):
            return
        time.sleep(0.2)
    raise SystemExit('prompt did not finish')


def powershell(command):
    return subprocess.run(['powershell', '-NoProfile', '-Command', command],
                          capture_output=True, text=True, timeout=60).stdout.strip()


def snapshot():
    gpu = powershell('nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits').splitlines()[0]
    # Every python process: the SAM 3 node may run in an isolated child of ComfyUI.
    rss = powershell("(Get-Process python* -ErrorAction SilentlyContinue | Measure-Object WorkingSet64 -Sum).Sum")
    private = powershell("(Get-Process python* -ErrorAction SilentlyContinue | Measure-Object PrivateMemorySize64 -Sum).Sum")
    return {'gpu_used_mb': int(float(gpu)), 'python_working_set_mb': round(int(rss) / 2**20),
            'python_private_mb': round(int(private) / 2**20)}


queue = call('/queue')
assert not queue['queue_running'] and not queue['queue_pending'], 'ComfyUI is busy'
filename = upload(IMAGE.read_bytes())
run('craft_ai_measure_warmup', filename)  # loads the model so it is not counted below
rows = [('after warm-up (model loaded, 1 extra entry)', snapshot())]
for index in range(1, 4):
    run(f'craft_ai_measure_{index}', filename)
    rows.append((f'after fresh entry {index}', snapshot()))
base = rows[0][1]
report = {'rows': [{'step': name, **values} for name, values in rows], 'per_entry_mb': {}}
last = rows[-1][1]
for key in base:
    report['per_entry_mb'][key] = round((last[key] - base[key]) / 3, 1)
text = scrub(json.dumps(report, indent=2))
(HERE / 'verification/sg3-slot-footprint.json').write_text(text, encoding='utf-8')
print(text)
sys.exit(0)
