"""Run the live session-ownership and cancellation tests against the person's ComfyUI.
Unlike run_crate_live.py this talks to the server directly, because the hover route must work.
It is safe for a running PhotoCraft: these sessions use the slot ids craft_ai_sam3_0..3 and never
the older shared craft_ai_sam3 id. It starts nothing, installs nothing and changes no
configuration. Everything saved passes through scrub().
"""
import json
import os
import subprocess
import urllib.request
from pathlib import Path

from scrub import scrub

URL = 'http://127.0.0.1:8000'
ROOT = Path(__file__).resolve().parents[3]
EVIDENCE = Path(__file__).resolve().parent / 'verification'
EVIDENCE.mkdir(exist_ok=True)


def get(path):
    with urllib.request.urlopen(URL + path, timeout=15) as response:
        return json.loads(response.read())


assert get('/system_stats')['system']['comfyui_version'], 'not ComfyUI'


def memory():
    stats = get('/system_stats')
    device = stats['devices'][0]
    return {'ram_free_mb': round(stats['system']['ram_free'] / 2**20),
            'vram_free_mb': round(device['vram_free'] / 2**20),
            'torch_vram_free_mb': round(device['torch_vram_free'] / 2**20)}


memory_before = memory()
queue = get('/queue')
assert not queue['queue_running'] and not queue['queue_pending'], 'ComfyUI is busy'
env = os.environ.copy()
env['CRAFT_TEST_COMFYUI_URL'] = URL
env['CRAFT_TEST_SAM3_IMAGE'] = str(Path(__file__).resolve().parent / 'evidence/astronaut-source.png')
command = ['cargo', 'test', '-p', 'craft-ai-comfyui', '--test', 'sam3_live_sessions', '-j', '8',
           '--', '--ignored', '--nocapture', '--test-threads=1']
completed = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True, timeout=400)
log = scrub(completed.stdout + completed.stderr)
(EVIDENCE / 'sg3-live-sessions.txt').write_text(log, encoding='utf-8')
print(log)
assert completed.returncode == 0, 'Live session verification failed'
after = get('/queue')
assert not after['queue_running'] and not after['queue_pending'], 'queue not empty afterwards'
summary = {'upstream': URL, 'real_inference': True, 'test_exit_code': completed.returncode,
           'queue_after': after, 'memory_before': memory_before, 'memory_after': memory()}
(EVIDENCE / 'sg3-live-summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
print(json.dumps(summary))
