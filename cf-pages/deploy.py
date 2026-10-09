#!/usr/bin/env python3
"""Deploy the indy-reads Worker (static via KV + API routes) to Cloudflare."""
import json
import os
import sys
import urllib.request
import uuid

sys.path.insert(0, "/opt/hatch/skills/skill-creator/bin")
from dynamic_credentials import add_surrogate_to_request, read_json_response

API = "https://api.cloudflare.com/client/v4"
HOSTS = ["api.cloudflare.com"]
ACCT = "243aabef9144fd3b8dd334cb19319e75"
NS = "999744a5a7c5476a9d646fbddcc183cf"
SCRIPT = "indy-reads"
WORKER = "/home/hatch/workspace/indy-reads/cf-pages/worker.js"
WASM = "/home/hatch/workspace/indy-reads/wasm/governor-wasm/governor_wasm.wasm"
LOADER = "/home/hatch/workspace/indy-reads/wasm/governor-wasm/loader.mjs"
SHELL = "/home/hatch/workspace/indy-reads/wasm/governor-wasm/shell-core.mjs"
STAGE = "/tmp/indy-deploy"


def stage_modules():
    """Stage flat modules for upload. The Scripts API rejects relative
    specifiers with '..', so the wasm helper modules ship as top-level
    parts and worker.js's imports are rewritten to match. The wasm itself
    ships as a module part (pre-compiled by the platform); runtime
    byte-compilation is disallowed on Workers."""
    import shutil
    shutil.rmtree(STAGE, ignore_errors=True)
    os.makedirs(STAGE, exist_ok=True)
    with open(WORKER) as f:
        worker = f.read()
    worker = worker.replace(
        "from '../wasm/governor-wasm/loader.mjs'", "from './loader.mjs'")
    worker = worker.replace(
        "from '../wasm/governor-wasm/shell-core.mjs'", "from './shell-core.mjs'")
    with open(os.path.join(STAGE, "worker.js"), "w") as f:
        f.write(worker)
    shutil.copy(LOADER, os.path.join(STAGE, "loader.mjs"))
    shutil.copy(SHELL, os.path.join(STAGE, "shell-core.mjs"))
    shutil.copy(WASM, os.path.join(STAGE, "governor_wasm.wasm"))
    return STAGE


def main():
    stage = stage_modules()
    with open(os.path.join(stage, "worker.js"), "rb") as f:
        code = f.read()
    with open(os.path.join(stage, "loader.mjs"), "rb") as f:
        loader = f.read()
    with open(os.path.join(stage, "shell-core.mjs"), "rb") as f:
        shell = f.read()
    with open(os.path.join(stage, "governor_wasm.wasm"), "rb") as f:
        wasm = f.read()
    metadata = {
        "main_module": "worker.js",
        "compatibility_date": "2026-10-01",
        "compatibility_flags": ["nodejs_compat"],
        "bindings": [
            {"type": "kv_namespace", "namespace_id": NS, "name": "ASSETS"},
            # D1 wired 2026-10-08 (repair): provisioned database indy-reads.
            # worker.js mirrors the receipt chain into receipts table
            # (best-effort, off hot path via ctx.waitUntil).
            {"type": "d1", "name": "INDY_D1", "id": "efb12a66-2a06-4ffb-9084-188589a23f85"},
        ],
    }
    boundary = uuid.uuid4().hex
    crlf = b"\r\n"

    def part(name, filename, ctype, data):
        return (b"--" + boundary.encode() + crlf
                + f'Content-Disposition: form-data; name="{name}"; filename="{filename}"'.encode() + crlf
                + f"Content-Type: {ctype}".encode() + crlf + crlf
                + data + crlf)

    body = b"".join([
        b"--" + boundary.encode() + crlf
        + b'Content-Disposition: form-data; name="metadata"' + crlf
        + b"Content-Type: application/json" + crlf + crlf
        + json.dumps(metadata).encode() + crlf,
        part("worker.js", "worker.js", "application/javascript+module", code),
        part("loader.mjs", "loader.mjs", "application/javascript+module", loader),
        part("shell-core.mjs", "shell-core.mjs", "application/javascript+module", shell),
        part("governor_wasm.wasm", "governor_wasm.wasm", "application/wasm", wasm),
        b"--" + boundary.encode() + b"--" + crlf,
    ])
    req = urllib.request.Request(
        f"{API}/accounts/{ACCT}/workers/scripts/{SCRIPT}",
        data=body, method="PUT",
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"})
    add_surrogate_to_request(req, "custom.cloudflare", allowed_hosts=HOSTS)
    r = read_json_response(urllib.request.urlopen(req, timeout=120))
    if not r.get("success"):
        sys.exit("ERR deploy: " + json.dumps(r.get("errors")))
    print("deployed:", f"https://{SCRIPT}.cutecity.workers.dev")


if __name__ == "__main__":
    main()
