// cf-pages/worker.js — Indy_Reads demo on Cloudflare Pages. THIN SHELL.
//
// Wave-24 (Zack: "why is it JavaScript... why not rust"): every verdict,
// veto, classification, receipt, and failover decision comes from the wasm
// governor — the REAL Rust src/governor.rs compiled to wasm32. This file
// does routing, wasm loading/invocation, and response sending ONLY. The
// two embedded hand-ports (turn IIFE + mcp IIFE, ~700 lines of normalize /
// claimsAuthority / classifyIntent / validateReply / receipt chain) are
// deleted; their logic lives in Rust.
//
// Presentation (persona lines, branch routing, theater) lives in the shared
// shell-core.mjs, also free of governor logic.
// Unrelated routes (health, tts, static KV serving) are unchanged.

import { loadGovernor } from '../wasm/governor-wasm/loader.mjs';
import { handleTurn, handleMcpGet, handleMcpPost, makeGenerate } from '../wasm/governor-wasm/shell-core.mjs';

async function sha256hex(s) {
  const d = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(s));
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

function json(data, status) {
  return new Response(JSON.stringify(data), {
    status: status || 200,
    headers: { 'content-type': 'application/json' },
  });
}

// ---- wasm governor (per isolate). The module is fetched from the site's
// ---- own origin (/governor_wasm.wasm.gz, shipped by kv-upload.py) and
// ---- gunzipped in-isolate. GOVERNOR_WASM_BYTES injects bytes for tests.
// ---- On any wasm throw the instance is dropped and recreated once.
let wasmP = null, gov = null, mcp = null;
function resetGov() { gov = null; mcp = null; }
async function loadWasmBytes(env, request) {
  if (env.GOVERNOR_WASM_BYTES) return env.GOVERNOR_WASM_BYTES;
  const r = await fetch(new URL('/governor_wasm.wasm.gz', request.url));
  if (!r.ok) throw new Error('wasm fetch ' + r.status);
  const gz = await r.arrayBuffer();
  const ds = new DecompressionStream('gzip');
  const stream = new Blob([gz]).stream().pipeThrough(ds);
  return new Uint8Array(await new Response(stream).arrayBuffer());
}
async function getHandles(env, request) {
  if (!wasmP) wasmP = loadWasmBytes(env, request).then((b) => loadGovernor(b));
  const wasm = await wasmP;
  if (!gov) gov = wasm.createGovernor();
  if (!mcp) mcp = wasm.createMcp();
  return { wasm, gov, mcp };
}

// ===== turn =====
async function turnHandler({ request, env }) {
  if (request.method !== 'POST') return json({ error: 'POST only' }, 405);
  let body = {};
  try { body = await request.json(); } catch {}
  const run = async () => {
    const h = await getHandles(env, request);
    const providers = [];
    if (env.ROUTER_KEY && String(env.ROUTER_KEY).trim())
      providers.push({ name: 'primary', generate: makeGenerate({ url: env.ROUTER_URL, key: env.ROUTER_KEY, label: 'primary' }) });
    if (env.ROUTER_KEY_2 && String(env.ROUTER_KEY_2).trim())
      providers.push({ name: 'secondary', generate: makeGenerate({ url: env.ROUTER_URL_2, key: env.ROUTER_KEY_2, label: 'secondary' }) });
    return handleTurn(h, { body, providers, sha256hex });
  };
  try {
    const out = await run();
    return json(out.json, out.status);
  } catch (e) {
    resetGov();
    try {
      const out = await run();
      return json(out.json, out.status);
    } catch (e2) {
      return json({ error: 'governor fault: ' + String((e2 && e2.message) || e2).slice(0, 160) }, 500);
    }
  }
}

// ===== health =====
async function healthHandler({ env }) {
  return json({
    service: 'indy-reads', status: 'ok', mode: 'fixture',
    router_key: !!(env.ROUTER_KEY && String(env.ROUTER_KEY).trim()),
    ts: Date.now(),
  });
}

// ===== receipts =====
async function receiptsHandler({ env, request }) {
  try {
    const h = await getHandles(env, request);
    const n = Math.min(200, Math.max(1, parseInt(new URL(request.url).searchParams.get('n') || '50', 10) || 50));
    const receipts = h.gov.recent_receipts(n);
    if (receipts && receipts.error) throw new Error(receipts.error);
    return json({ receipts, note: 'per-isolate chain; resets on cold start' });
  } catch (e) {
    resetGov();
    return json({ error: 'governor fault: ' + String((e && e.message) || e).slice(0, 120) }, 500);
  }
}

// ===== mcp =====
async function mcpHandler({ env, request }) {
  let h;
  try {
    h = await getHandles(env, request);
  } catch (e) {
    return json({ error: 'governor wasm failed to load: ' + String(e.message || e).slice(0, 120) }, 500);
  }
  if (request.method === 'GET') {
    const out = handleMcpGet(h);
    return json(out.json, out.status);
  }
  if (request.method === 'POST') {
    let body = {};
    try { body = await request.json(); } catch {}
    const out = handleMcpPost(h, body);
    return json(out.json, out.status);
  }
  return json({ error: 'method not allowed' }, 405);
}

// ===== tts (unchanged: pure proxy, no governor involvement) =====
async function ttsHandler({ request }) {
  try {
    const q = String(new URL(request.url).searchParams.get('q') || '').slice(0, 200);
    if (!q) return new Response('missing q', { status: 400 });
    const url = 'https://translate.google.com/translate_tts?ie=UTF-8&tl=en&client=tw-ob&q=' + encodeURIComponent(q);
    const r = await fetch(url, { headers: { 'User-Agent': 'Mozilla/5.0 (Linux; Android 10)' } });
    if (!r.ok) return new Response('tts upstream ' + r.status, { status: 502 });
    const ab = await r.arrayBuffer();
    if (ab.byteLength < 100) return new Response('tts empty', { status: 502 });
    return new Response(ab, {
      headers: {
        'Content-Type': 'audio/mpeg',
        'Cache-Control': 'public, max-age=86400',
      },
    });
  } catch (e) {
    return new Response('tts error', { status: 500 });
  }
}

// ---------- static assets from KV ----------
const KV_KEY_MAP = {
  "/": "index.html",
  "/index.html": "index.html",
};

async function serveStatic(pathname, env) {
  let key = KV_KEY_MAP[pathname] || pathname.replace(/^\//, "");
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
  if (/\.(js|css|webp|png|wasm)(\.gz)?$/.test(key)) headers["cache-control"] = "public, max-age=86400";
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
