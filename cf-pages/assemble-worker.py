#!/usr/bin/env python3
"""Assemble cf-pages/functions/api/*.js into a single Cloudflare Worker script."""
import os
import re

SRC = "/home/hatch/workspace/indy-reads/cf-pages/functions/api"
OUT = "/home/hatch/workspace/indy-reads/cf-pages/worker.js"

HANDLERS = [
    # ("turn", "turn.js") REMOVED 2026-10-08: the hand-ported turn.js was a
    # stale second truth source (demo-only behavior not in the Rust core).
    # /api/turn now routes through the wasm governor (../wasm/governor-wasm/)
    # in worker.js — the REAL Rust src/governor.rs compiled to wasm32.
    ("health", "health.js"),
    ("receipts", "receipts.js"),
    ("mcp", "mcp.js"),
    ("tts", "tts.js"),
]

WORKER_TAIL = '''
// ---------- static assets from KV ----------
const KV_KEY_MAP = {
  "/": "index.html",
  "/index.html": "index.html",
};

async function serveStatic(pathname, env) {
  let key = KV_KEY_MAP.get(pathname) || pathname.replace(/^\\//, "");
  if (!key) key = "index.html";
  // block dotfiles and api paths from static serving
  if (key.startsWith(".") || key.includes("/.")) {
    return new Response("not found", { status: 404 });
  }
  const entry = await env.ASSETS.getWithMetadata(key, { type: "arrayBuffer" });
  if (!entry || !entry.value) {
    // SPA fallback for unknown non-api paths
    if (!pathname.startsWith("/api/")) {
      const idx = await env.ASSETS.getWithMetadata("index.html", { type: "arrayBuffer" });
      if (idx && idx.value) {
        return new Response(idx.value, { headers: { "content-type": "text/html; charset=utf-8" } });
      }
    }
    return new Response("not found", { status: 404 });
  }
  const ct = (entry.metadata && entry.metadata.ct) || "application/octet-stream";
  const headers = { "content-type": ct };
  if (key.endsWith(".wasm.gz")) headers["content-encoding"] = "gzip";
  if (/\\.(js|css|webp|png|wasm)(\\.gz)?$/.test(key)) headers["cache-control"] = "public, max-age=86400";
  return new Response(entry.value, { headers });
}

// ---------- router ----------
const API_HANDLERS = { turn: turnHandler, health: healthHandler, receipts: receiptsHandler, mcp: mcpHandler, tts: ttsHandler };

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    const pathname = url.pathname;
    if (pathname.startsWith("/api/")) {
      const route = pathname.slice(5).split("/")[0].split(".")[0];
      const h = API_HANDLERS[route];
      if (h) {
        try {
          return await h({ request, env, params: {} });
        } catch (e) {
          return new Response(JSON.stringify({ error: "worker: " + e.message }), {
            status: 500, headers: { "content-type": "application/json" },
          });
        }
      }
      return new Response(JSON.stringify({ error: "unknown api route" }), {
        status: 404, headers: { "content-type": "application/json" },
      });
    }
    return serveStatic(pathname, env);
  },
};
'''


def main():
    parts = ["import { createHash, randomBytes } from 'node:crypto';\n"]
    for name, fn in HANDLERS:
        with open(os.path.join(SRC, fn)) as f:
            src = f.read()
        # drop the node:crypto import (hoisted to top)
        src = re.sub(r"^import \{[^}]*\} from 'node:crypto';\s*\n", "", src, flags=re.M)
        # strip the export keyword; wrap the whole file in a scope so
        # top-level names (json, sha256, state, ...) can't collide
        src = src.replace("export async function onRequest",
                          "async function onRequest", 1)
        parts.append(f"\n// ===== {fn} =====\n"
                     f"const {name}Handler = (() => {{\n{src}\nreturn onRequest;\n}})();")
    parts.append(WORKER_TAIL)
    with open(OUT, "w") as f:
        f.write("\n".join(parts))
    print(f"wrote {OUT} ({os.path.getsize(OUT)} bytes)")


if __name__ == "__main__":
    main()
