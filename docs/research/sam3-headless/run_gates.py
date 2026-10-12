"""Record targeted, serial verification for the sg1 shared-crate slice."""
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent / 'verification'
OUT.mkdir(exist_ok=True)
COMMANDS = [
    ('format', ['cargo', 'fmt', '-p', 'craft-ai-comfyui', '--', '--check']),
    ('comfyui-tests', ['cargo', 'test', '-p', 'craft-ai-comfyui', '-j', '8']),
    ('client-comfyui-tests', ['cargo', 'test', '-p', 'craft-ai-client', '--test', 'comfyui', '-j', '8']),
    ('clippy', ['cargo', 'clippy', '-p', 'craft-ai-comfyui', '-j', '8', '--all-targets', '--', '-D', 'warnings']),
    ('model-wasm', ['cargo', 'check', '-p', 'craft-ai-model', '-j', '8', '--target', 'wasm32-unknown-unknown']),
]
results = []
for name, command in COMMANDS:
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=300)
    log = result.stdout + result.stderr
    (OUT / (name + '.txt')).write_text(log, encoding='utf-8')
    row = {'gate': name, 'command': command, 'exit_code': result.returncode}
    summaries = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    if summaries:
        row['passed'] = sum(int(passed) for passed, _, _ in summaries)
        row['ignored'] = sum(int(ignored) for _, _, ignored in summaries)
    results.append(row)
    (OUT / 'sg1-gates.json').write_text(json.dumps(results, indent=2), encoding='utf-8')
    print(json.dumps(row), flush=True)
    if result.returncode:
        print(log)
        raise SystemExit(result.returncode)
print('All targeted gates passed')
