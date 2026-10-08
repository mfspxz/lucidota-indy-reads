#!/usr/bin/env python3
"""Tight Vercel deploy loop for indy-reads-ui.

Usage: vercel-loop.py [--force]
  1. Hashes local web/ files.
  2. Fetches the same files from the production alias and compares.
  3. If identical (and not --force): prints IN SYNC, deploys nothing.
  4. If different: creates a production deployment via the Vercel MCP CLI,
     waits for READY, then re-verifies content hashes against the alias.

Answers one question per run: is production serving exactly what's in web/?
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

# deploy path -> local path. Static assets are GET-hashable (sync check);
# API_FILES are serverless functions (deploy-only: GET would execute them).
FILES = {
    "index.html": "web/index.html",
    "app.js": "web/app.js",
    "style.css": "web/style.css",
    "live.js": "web/live.js",
    "voice.html": "web/voice.html",
    "voice.js": "web/voice.js",
    "prosody.js": "web/prosody.js",
    "governor-wasm-loader.js": "web/governor-wasm-loader.js",
    "governor_wasm.wasm.gz": "web/governor_wasm.wasm.gz",
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
# vercel.json is project config (consumed at deploy time via projectSettings),
# NOT a served asset — it 404s by design, so it must not join the sync check.
CONFIG_FILES = {"vercel.json": "vercel.json"}


def sha1_file(path):
    h = hashlib.sha1()
    with open(path, "rb") as f:
        h.update(f.read())
    return h.hexdigest()


def sha1_url(url):
    h = hashlib.sha1()
    with urllib.request.urlopen(url, timeout=20) as r:
        if r.status != 200:
            return None
        h.update(r.read())
    return h.hexdigest()


def minify_js(path):
    """Minify JS for deploy payload only; local source stays intact."""
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


def ensure_wasm_gz():
    """Deterministic gzip of the governor wasm (mtime=0 -> stable bytes,
    so the sync check stays meaningful). Returns the gzipped bytes."""
    with open(f"{ROOT}/web/governor_wasm.wasm", "rb") as f:
        raw = f.read()
    buf = io.BytesIO()
    with gzip.GzipFile(fileobj=buf, mode="wb", compresslevel=9, mtime=0) as gzf:
        gzf.write(raw)
    gz = buf.getvalue()
    with open(f"{ROOT}/web/governor_wasm.wasm.gz", "wb") as f:
        f.write(gz)
    return gz


def vcall(tool, args):
    out = subprocess.run(
        ["vercel", "call-tool", "--name", tool,
         "--arguments-json", json.dumps(args)],
        capture_output=True, text=True, timeout=120)
    d = json.loads(out.stdout)
    return json.loads(d["result"]["content"][0]["text"])["result"]


def main():
    force = "--force" in sys.argv
    ensure_wasm_gz()  # deterministic build artifact; keeps the sync check honest
    stage = {k: staged_bytes(v) for k, v in FILES.items()}
    local = {k: hashlib.sha1(stage[k]).hexdigest() for k in FILES}
    print("local files:", len(local))
    if not force:
        remote = {}
        for k in FILES:
            try:
                remote[k] = sha1_url(f"{ALIAS}/{k}")
            except Exception:
                remote[k] = None
        diffs = [k for k in FILES if local[k] != remote.get(k)]
        if not diffs:
            print("IN SYNC — production serves exactly web/. No deploy.")
            return 0
        print("DIFFERS:", ", ".join(diffs))
    else:
        print("--force: skipping sync check")

    payload_files = []
    for deploy_path, local_path in {**FILES, **API_FILES, **CONFIG_FILES}.items():
        if deploy_path == "governor_wasm.wasm.gz":
            continue  # uploaded to the file store, referenced by sha below
        if deploy_path in stage:
            data = stage[deploy_path]
        else:
            data = staged_bytes(local_path)
        enc = "utf-8"
        try:
            txt = data.decode("utf-8")
        except UnicodeDecodeError:
            txt, enc = base64.b64encode(data).decode(), "base64"
        payload_files.append({"file": deploy_path, "data": txt, "encoding": enc})

    # WASM governor: 157KB raw blows the CLI's 128KB single-argument limit,
    # so it rides the file store (gzipped) and is referenced by sha.
    # The browser loader fetches governor_wasm.wasm.gz and gunzips it.
    gzdata = ensure_wasm_gz()
    gsha = hashlib.sha1(gzdata).hexdigest()
    vcall("upload_file", {"xVercelDigest": gsha, "contentLength": len(gzdata),
                          "requestBody": base64.b64encode(gzdata).decode()})
    payload_files.append({"file": "governor_wasm.wasm.gz", "sha": gsha, "size": len(gzdata)})
    print(f"wasm uploaded by sha {gsha[:12]} ({len(gzdata)} bytes gz)")

    # Bundled router key: injected at deploy time from the key manager.
    # Never written to the repo, never sync-checked, never served —
    # vercel.json includeFiles pulls it into the turn.js function bundle.
    try:
        with open("/home/hatch/workspace/keys/ROUTER_KEY") as f:
            _rk = f.read().strip()
        if _rk:
            payload_files.append({"file": "api/.router_key", "data": _rk, "encoding": "utf-8"})
            print("injected api/.router_key from key manager")
    except OSError:
        print("no local ROUTER_KEY; deploying without bundled key")

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
        time.sleep(5)
    print("READY. Waiting for alias cutover + edge propagation...")
    time.sleep(15)
    bad = []
    for attempt in range(3):
        bad = []
        for k in FILES:
            try:
                if sha1_url(f"{ALIAS}/{k}") != local[k]:
                    bad.append(k)
            except Exception as e:
                bad.append(f"{k} ({e})")
        if not bad:
            break
        time.sleep(10)
    if bad:
        print("VERIFY FAILED:", ", ".join(bad))
        return 1
    print(f"VERIFIED — {ALIAS} serves the new bundle.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
