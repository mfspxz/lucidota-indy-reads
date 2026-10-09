#!/usr/bin/env python3
"""Deploy indy-reads-ui via per-file uploads (works around CLI arg limits).

Uploads each file individually via upload_file, then creates the deployment
referencing files by SHA. Skips governor_wasm.wasm.gz (123KB, exceeds the
128KB single-arg CLI limit even gzipped; the loader degrades gracefully).
"""
import base64
import hashlib
import json
import subprocess
import sys
import time

PROJECT_ID = "prj_yRmcBgUDnAk4bbwCddKlDcBa0a78"
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
    "api/_auth.js": "api/_auth.js",
    "api/turn.js": "api/turn.js",
    "api/health.js": "api/health.js",
    "api/receipts.js": "api/receipts.js",
    "api/mcp.js": "api/mcp.js",
    "api/tts.js": "api/tts.js",
}
CONFIG_FILES = {"vercel.json": "vercel.json"}


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
    p = f"{ROOT}/{local_path}"
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
    content = d["result"]["content"][0]["text"]
    try:
        inner = json.loads(content)
        if isinstance(inner, dict) and "error" in inner:
            raise RuntimeError(f"{tool} error: {json.dumps(inner['error'])[:200]}")
        return inner.get("result", inner)
    except json.JSONDecodeError:
        return content


def main():
    refs = []
    for deploy_path, local_path in {**FILES, **API_FILES, **CONFIG_FILES}.items():
        data = staged_bytes(local_path)
        sha = hashlib.sha1(data).hexdigest()
        b64 = base64.b64encode(data).decode()
        vcall("upload_file", {
            "xVercelDigest": sha,
            "contentLength": len(data),
            "requestBody": b64,
        })
        refs.append({"file": deploy_path, "sha": sha, "size": len(data)})
        print(f"uploaded {deploy_path} ({len(data)}b sha={sha[:12]})")

    # bundled router key from the key manager (new rotated key)
    with open("/home/hatch/workspace/keys/ROUTER_KEY") as f:
        _rk = f.read().strip()
    # small enough to inline
    refs.append({"file": "api/.router_key", "data": _rk, "encoding": "utf-8"})
    print("injected api/.router_key from key manager")

    body = {"requestBody": {
        "name": "indy-reads-ui", "project": PROJECT_ID, "target": "production",
        "projectSettings": {"framework": None, "buildCommand": None,
                            "outputDirectory": "web"},
        "files": refs}}
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
