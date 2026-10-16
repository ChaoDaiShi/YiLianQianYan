"""Temporary CI diagnosis using only synthetic ACP fixture requests."""
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time

source = Path("backend/tests/acp_adapter.rs").read_text(encoding="utf-8-sig")
offset = source.index("fn powershell_fixture()")
begin = source.index('r#"', offset) + 3
body = source[begin:source.index('"#', begin)]
root = Path(os.environ["TEMP"]) / "yilian-acp-ci-probe"
root.mkdir(exist_ok=True)
script = root / "fixture.ps1"
script.write_text(body, encoding="utf-8")
executable = Path(os.environ["SystemRoot"]) / "System32/WindowsPowerShell/v1.0/powershell.exe"
clean_env = {key: os.environ[key] for key in ("SystemRoot", "WINDIR", "TEMP", "TMP") if key in os.environ}


def probe(label, env, flags):
    started = time.monotonic()
    process = subprocess.Popen(
        [str(executable), "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(script)],
        cwd=root, env=env, creationflags=flags,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    output = queue.Queue()
    errors = []
    threading.Thread(target=lambda: [output.put(line) for line in iter(process.stdout.readline, b"")], daemon=True).start()
    threading.Thread(target=lambda: errors.extend(iter(process.stderr.readline, b"")), daemon=True).start()
    results = []
    try:
        for request in (
            {"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": 1}},
            {"jsonrpc": "2.0", "id": 1, "method": "session/new", "params": {"cwd": str(root)}},
        ):
            process.stdin.write((json.dumps(request, ensure_ascii=False) + "\n").encode("utf-8"))
            process.stdin.flush()
            try:
                results.append(output.get(timeout=8).decode("utf-8", errors="replace"))
            except queue.Empty:
                results.append("TIMEOUT:" + request["method"])
                break
    finally:
        process.kill()
        process.wait(timeout=5)
        time.sleep(0.1)
    print(json.dumps({"case": label, "seconds": round(time.monotonic() - started, 2), "responses": results, "stderr": b"".join(errors).decode("utf-8", errors="replace")}, ensure_ascii=True), flush=True)


for label, keys in (
    ("path", ("PATH",)),
    ("profile", ("USERPROFILE", "HOMEDRIVE", "HOMEPATH", "APPDATA", "LOCALAPPDATA")),
    ("modules", ("PSModulePath",)),
    ("system", ("SystemDrive", "COMSPEC")),
):
    environment = dict(clean_env)
    environment.update({key: os.environ[key] for key in keys if key in os.environ})
    probe(label, environment, subprocess.CREATE_NO_WINDOW)
