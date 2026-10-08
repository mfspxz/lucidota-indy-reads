#!/usr/bin/env python3
"""Upload indy-reads static assets to the indy-reads-assets KV namespace."""
import base64
import json
import mimetypes
import os
import sys
import urllib.request
import urllib.error

sys.path.insert(0, "/opt/hatch/skills/skill-creator/bin")
from dynamic_credentials import add_surrogate_to_request, read_json_response

ACCT = "243aabef9144fd3b8dd334cb19319e75"
NS = "999744a5a7c5476a9d646fbddcc183cf"
WEB = "/home/hatch/workspace/indy-reads/web"
API = "https://api.cloudflare.com/client/v4"

FILES = [
    "index.html", "app.js", "style.css", "live.js",
    "voice.html", "voice.js", "prosody.js", "demo.js",
    "governor-wasm-loader.js", "governor_wasm.wasm.gz",
    "art/indy-portrait.webp", "art/indy/prop-notebook.webp",
]

CT = {
    ".html": "text/html; charset=utf-8",
    ".js": "application/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".webp": "image/webp",
    ".png": "image/png",
    ".wasm": "application/wasm",
    ".gz": "application/gzip",
}


def content_type(path):
    if path.endswith(".wasm.gz"):
        return "application/wasm"
    ext = os.path.splitext(path)[1]
    return CT.get(ext, "application/octet-stream")


def api_req(method, path, data=None, raw=False):
    url = API + path
    body = data if raw else (json.dumps(data).encode() if data else None)
    req = urllib.request.Request(url, data=body, method=method,
                                 headers={"Content-Type": "application/json"})
    add_surrogate_to_request(req, "custom.cloudflare", allowed_hosts=["api.cloudflare.com"])
    return read_json_response(urllib.request.urlopen(req, timeout=60))


def main():
    items = []
    for f in FILES:
        p = os.path.join(WEB, f)
        if not os.path.exists(p):
            print(f"SKIP missing: {f}", file=sys.stderr)
            continue
        with open(p, "rb") as fh:
            raw = fh.read()
        try:
            val = raw.decode("utf-8")
            b64 = False
        except UnicodeDecodeError:
            val = base64.b64encode(raw).decode()
            b64 = True
        items.append({"key": f, "value": val, "base64": b64,
                      "metadata": {"ct": content_type(f)}})
    print(f"uploading {len(items)} files...")
    r = api_req("PUT", f"/accounts/{ACCT}/storage/kv/namespaces/{NS}/bulk", items)
    print("success:", r.get("success"))
    if not r.get("success"):
        print(json.dumps(r)[:500])


if __name__ == "__main__":
    sys.exit(main())
