// api/turn.js — Indy_Reads demo backend (Vercel serverless). THIN SHELL.
//
// Wave-24 (Zack: "why is it JavaScript... why not rust"): every verdict,
// veto, classification, receipt, and failover decision comes from the wasm
// governor — the REAL Rust src/governor.rs compiled to wasm32. This file
// does request parsing, wasm loading/invocation, and response sending ONLY.
// The 426-line hand-port (normalize, claimsAuthority, classifyIntent,
// validateReply, receipt chain) is deleted; its logic lives in Rust.
//
// Presentation (persona lines, branch routing, theater) lives in the shared
// shell-core.mjs, also free of governor logic.

const crypto = require('crypto');
const path = require('path');

const sha256hex = (s) => crypto.createHash('sha256').update(s, 'utf8').digest('hex');

// ---- wasm governor (per function instance; same lifecycle as the old
// ---- in-memory state). On any wasm throw the instance is dropped and
// ---- recreated once (a poisoned mutex never recovers).
let wasmPromise = null;
function getWasm() {
  if (!wasmPromise) {
    wasmPromise = (async () => {
      const { loadNodeGovernor } = await import('../wasm/governor-wasm/loader.mjs');
      return loadNodeGovernor();
    })();
  }
  return wasmPromise;
}
let govInstance = null;
let mcpInstance = null;
function resetGov() {
  try { govInstance && govInstance.free(); } catch {}
  try { mcpInstance && mcpInstance.free(); } catch {}
  govInstance = null; mcpInstance = null;
}

// Router keys: env first, then the bundled file (vercel.json includeFiles
// pulls api/.router_key into the bundle; injected at deploy time, never
// committed). Bundled file wins: the dashboard env holds a stale key.
function bundledRouterKey() {
  const fs = require('fs');
  const cands = [path.join(__dirname, '.router_key'), path.join(process.cwd(), 'api', '.router_key')];
  for (const p of cands) {
    try { const k = fs.readFileSync(p, 'utf8').trim(); if (k) return k; } catch {}
  }
  return null;
}

module.exports = async (req, res) => {
  if (req.method !== 'POST') { res.status(405).json({ error: 'POST only' }); return; }
  let body = req.body;
  if (typeof body === 'string') { try { body = JSON.parse(body); } catch { body = {}; } }
  body = body || {};

  let core;
  try {
    core = await import('../wasm/governor-wasm/shell-core.mjs');
  } catch (e) {
    res.status(500).json({ error: 'shell core failed to load: ' + String(e.message || e).slice(0, 120) });
    return;
  }

  const run = async () => {
    const h = await getWasmHandles();
    const routerKey = bundledRouterKey() || process.env.ROUTER_KEY;
    const providers = [];
    if (routerKey) providers.push({ name: 'primary', generate: core.makeGenerate({ url: process.env.ROUTER_URL, key: routerKey, label: 'primary' }) });
    if (process.env.ROUTER_KEY_2) providers.push({ name: 'secondary', generate: core.makeGenerate({ url: process.env.ROUTER_URL_2, key: process.env.ROUTER_KEY_2, label: 'secondary' }) });
    return core.handleTurn(h, { body, providers, sha256hex });
  };

  try {
    const out = await run();
    res.status(out.status).json(out.json);
  } catch (e) {
    // Wasm throw: drop the (possibly poisoned) instance, retry once.
    resetGov();
    try {
      const out = await run();
      res.status(out.status).json(out.json);
    } catch (e2) {
      res.status(500).json({ error: 'governor fault: ' + String((e2 && e2.message) || e2).slice(0, 160) });
    }
  }
};

// Shared by api/receipts.js and api/mcp.js: same wasm module, same instance
// lifecycle.
async function getWasmHandles() {
  const wasm = await getWasm();
  if (!govInstance) govInstance = wasm.createGovernor();
  if (!mcpInstance) mcpInstance = wasm.createMcp();
  return { wasm, gov: govInstance, mcp: mcpInstance, resetGov };
}
module.exports.getWasmHandles = getWasmHandles;
