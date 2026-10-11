"""Start Jeremy's existing ComfyUI Desktop server without Electron.
No install, config edit or browser window. Log is in the Hermes scratch directory.
"""
import os
import subprocess
from pathlib import Path

home = Path(r"C:\Users\jerem")
base = home / "Documents" / "ComfyUI"
program = home / "AppData" / "Local" / "Programs" / "ComfyUI" / "resources" / "ComfyUI"
config = home / "AppData" / "Roaming" / "ComfyUI" / "extra_models_config.yaml"
python = base / ".venv" / "Scripts" / "python.exe"
log = home / "AppData" / "Local" / "hermes" / "cache" / "scratch" / "comfy-headless.log"
for p in (base, program / "main.py", config, python):
    if not p.exists():
        raise SystemExit(f"Missing existing ComfyUI path: {p}")
argv = [
    str(python), str(program / "main.py"),
    "--user-directory", str(base / "user"),
    "--input-directory", str(base / "input"),
    "--output-directory", str(base / "output"),
    "--front-end-root", str(program / "web_custom_versions" / "desktop_app"),
    "--base-directory", str(base),
    "--database-url", f"sqlite:///{(base / 'user' / 'comfyui.db').as_posix()}",
    "--extra-model-paths-config", str(config),
    "--log-stdout", "--disable-auto-launch", "--listen", "127.0.0.1",
    "--port", "8000", "--enable-manager",
]
with log.open("wb") as stream:
    proc = subprocess.Popen(
        argv, cwd=program, stdin=subprocess.DEVNULL, stdout=stream,
        stderr=subprocess.STDOUT,
        creationflags=subprocess.CREATE_NO_WINDOW | subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP,
    )
print(f"Started existing ComfyUI headless, parent PID {proc.pid}; log {log}")
