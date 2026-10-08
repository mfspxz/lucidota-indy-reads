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


def main():
    with open(WORKER) as f:
        code = f.read()
    metadata = {
        "main_module": "worker.js",
        "compatibility_date": "2026-10-01",
        "compatibility_flags": ["nodejs_compat"],
        "bindings": [{"type": "kv_namespace", "namespace_id": NS, "name": "ASSETS"}],
    }
    boundary = uuid.uuid4().hex
    parts = [
        f'--{boundary}\r\nContent-Disposition: form-data; name="metadata"\r\n'
        f'Content-Type: application/json\r\n\r\n{json.dumps(metadata)}\r\n',
        f'--{boundary}\r\nContent-Disposition: form-data; name="worker.js"; '
        f'filename="worker.js"\r\n'
        f'Content-Type: application/javascript+module\r\n\r\n{code}\r\n',
        f'--{boundary}--\r\n',
    ]
    body = "".join(parts).encode()
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
