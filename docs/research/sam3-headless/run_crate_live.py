"""Run the real Rust mask-batch test without overwriting PhotoCraft's hover cache.
A short-lived loopback proxy forwards real requests and rewrites only the test's
interactive node ID. It does not fabricate ComfyUI responses, start ComfyUI,
install anything, change configuration or follow redirects.
"""
import http.server
import json
import os
import subprocess
import threading
import urllib.error
import urllib.request
from pathlib import Path

UPSTREAM = 'http://127.0.0.1:8000'
ROOT = Path(__file__).resolve().parents[3]
EVIDENCE = Path(__file__).resolve().parent / 'verification'
EVIDENCE.mkdir(exist_ok=True)

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None

OPENER = urllib.request.build_opener(NoRedirect())

def upstream(path, data=None, headers=None):
    request = urllib.request.Request(UPSTREAM + path, data=data, headers=headers or {})
    try:
        with OPENER.open(request, timeout=180) as response:
            return response.status, response.headers.get('Content-Type', 'application/octet-stream'), response.read()
    except urllib.error.HTTPError as error:
        return error.code, 'application/json', error.read()

class Proxy(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def forward(self):
        if not self.path.startswith(('/system_stats', '/object_info', '/upload/image', '/prompt', '/history/', '/view?')):
            self.send_error(404)
            return
        data = None
        headers = {}
        if self.command == 'POST':
            size = int(self.headers.get('Content-Length', '0'))
            if size > 20 * 1024 * 1024:
                self.send_error(413)
                return
            data = self.rfile.read(size)
            headers['Content-Type'] = self.headers.get('Content-Type', 'application/json')
            if self.path == '/prompt':
                body = json.loads(data)
                graph = body['prompt']
                old_id = next((key for key, value in graph.items()
                               if value['class_type'] == 'SAM3InteractiveCollector'), None)
                if old_id is not None:
                    new_id = 'craft_ai_sg1_validation'
                    graph[new_id] = graph.pop(old_id)
                    for value in graph.values():
                        for name, item in value.get('inputs', {}).items():
                            if isinstance(item, list) and len(item) == 2 and item[0] == old_id:
                                value['inputs'][name] = [new_id, item[1]]
                data = json.dumps(body).encode()
        status, content_type, body = upstream(self.path, data, headers)
        self.send_response(status)
        self.send_header('Content-Type', content_type)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    do_GET = forward
    do_POST = forward

status, _, body = upstream('/system_stats')
assert status == 200 and json.loads(body)['system']['comfyui_version']
status, _, body = upstream('/queue')
queue = json.loads(body)
assert status == 200 and not queue['queue_running'] and not queue['queue_pending'], 'ComfyUI is busy'
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Proxy)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
env = os.environ.copy()
env['CRAFT_TEST_COMFYUI_URL'] = f'http://127.0.0.1:{server.server_port}'
env['CRAFT_TEST_SAM3_IMAGE'] = str(Path(__file__).resolve().parent / 'evidence/astronaut-source.png')
try:
    command = ['cargo', 'test', '-p', 'craft-ai-comfyui', '--test', 'sam3_masks_live', '-j', '8',
               '--', '--ignored', '--nocapture', '--test-threads=1']
    completed = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True, timeout=300)
    log = completed.stdout + completed.stderr
    (EVIDENCE / 'sg1-live-rust.txt').write_text(log, encoding='utf-8')
    print(log)
    assert completed.returncode == 0, 'Real Rust/ComfyUI verification failed'
    status, _, body = upstream('/queue')
    queue = json.loads(body)
    assert status == 200 and not queue['queue_running'] and not queue['queue_pending']
    result = {'upstream': UPSTREAM, 'real_inference': True, 'test_exit_code': completed.returncode,
              'hover_cache_isolated': True, 'isolated_node_id': 'craft_ai_sg1_validation', 'queue_after': queue}
    (EVIDENCE / 'sg1-live-summary.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result))
finally:
    server.shutdown()
    server.server_close()
