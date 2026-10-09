#!/usr/bin/env python3
"""Deploy indy-reads-ui, working around the upload_file CLI arg limit.

Same as vercel-loop.py --force, but references the deterministic wasm gzip
by SHA instead of uploading it (content-addressed, already on Vercel's file
store from the previous deploy).
"""
import base64
import gzip
import hashlib
import io
import json
import subprocess
import sys
import time
import urllib.request

PROJECT_ID = "prj_yRmcBgUDnAk4bbwCddKlDcBa0a78"
ALIAS = "https://indy-reads-ui-maroonedpilot-8906.vercel.app"
ROOT = "/home/hatch/workspace/indy-reads"

FILES = {
    "index.html": "web/index.html",
    "app.js": "web/app.js",
    "style.css": "web/style.css",
    "live.js": "web/live.js",
    "voice.html": "web/voice.html",
    "voice.js": "web/voice.js",
    "prosody.js": "web/prosody.js",
    "governor-wasm-loader.js": "web/governor-wasm-loader.js",
    "demo.js": "web/demo.js",
    "art/indy-portrait.webp": "web/art/indy-portrait.webp",
    "art/indy/prop-notebook.webp": "web/art/indy/prop-notebook.webp",
}
API_FILES = {
    "api/turn.js": "api/turn.js",
    "api/health.js": "api/health.js",
    "api/receipts.js": "api/receipts.js",
    "api/mcp.js": "api/mcp.js",
    "api/tts.js": "api/tts.js",
}
CONFIG_FILES = {"vercel.json": "vercel.json"}


def sha1_file(path):
    h = hashlib.sha1()
    with open(path, "rb") as f:
        h.update(f.read())
    return h.hexdigest()


def minify_js(path):
    try:
        out = subprocess.run(["npx", "-y", "terser", path, "--compress", "--mangle"],
                             capture_output=True, text=True, timeout=90)
        if out.returncode == 0 and out.stdout.strip():
            return out.stdout.encode()
    except Exception:
        pass
    with open(path, "rb") as f:
        return f.read()


def staged_bytes(local_path):
    p = local_path if local_path.startswith("/") else f"{ROOT}/{local_path}"
    if p.endswith(".js"):
        return minify_js(p)
    with open(p, "rb") as f:
        return f.read()


def vcall(tool, args):
    out = subprocess.run(
        ["vercel", "call-tool", "--name", tool,
         "--arguments-json", json.dumps(args)],
        capture_output=True, text=True, timeout=120)
    d = json.loads(out.stdout)
    return json.loads(d["result"]["content"][0]["text"])["result"]


def main():
    # deterministic wasm gzip (same bytes as previous deploy -> same sha)
    with open(f"{ROOT}/web/governor_wasm.wasm", "rb") as f:
        raw = f.read()
    buf = io.BytesIO()
    with gzip.GzipFile(fileobj=buf, mode="wb", compresslevel=9, mtime=0) as gzf:
        gzf.write(raw)
    gzdata = buf.getvalue()
    gsha = hashlib.sha1(gzdata).hexdigest()
    print(f"wasm sha {gsha[:12]} ({len(gzdata)} bytes), referenced not uploaded")

    payload_files = []
    for deploy_path, local_path in {**FILES, **API_FILES, **CONFIG_FILES}.items():
        data = staged_bytes(local_path)
        enc = "utf-8"
        try:
            txt = data.decode("utf-8")
        except UnicodeDecodeError:
            txt, enc = base64.b64encode(data).decode(), "base64"
        payload_files.append({"file": deploy_path, "data": txt, "encoding": enc})
    payload_files.append({"file": "governor_wasm.wasm.gz", "sha": gsha, "size": len(gzdata)})

    # bundled router key from the key manager (new rotated key)
    with open("/home/hatch/workspace/keys/ROUTER_KEY") as f:
        _rk = f.read().strip()
    payload_files.append({"file": "api/.router_key", "data": _rk, "encoding": "utf-8"})
    print("injected api/.router_key from key manager")

    body = {"requestBody": {
        "name": "indy-reads-ui", "project": PROJECT_ID, "target": "production",
        "projectSettings": {"framework": None, "buildCommand": None,
                            "outputDirectory": "web"},
        "files": payload_files}}
    dep = vcall("create_deployment", body)["deployment"]
    did, durl = dep["id"], dep["url"]
    print(f"deployment {did} -> https://{durl}")

    for _ in range(30):
        st = vcall("get_deployment", {"idOrUrl": did}).get("readyState")
        if st == "READY":
            break
        if st in ("ERROR", "CANCELED"):
            print(f"DEPLOY FAILED: {st}")
            return 1
        time.sleep(10)
    else:
        print("DEPLOY TIMEOUT waiting for READY")
        return 1
    print("READY")
    return 0


if __name__ == "__main__":
    sys.exit(main())
