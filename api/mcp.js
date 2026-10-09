// api/mcp.js — Indy_Reads MCP surface as serverless REST. THIN SHELL.
//
// Wave-24: the 12 tools run in the wasm module (the REAL src/mcp.rs,
// compiled — claims, evidence, epistemics, receipts). This file does
// JSON-RPC parsing and response sending ONLY. The 260-line hand-port
// (normalizeForMatch, claimsAuthority, classifyIntentGov, commitReceipt,
// assessClaim) is deleted; its logic lives in Rust.
//
// GET  /api/mcp            -> the 12 tool descriptors
// POST /api/mcp {tool, arguments} -> executes, returns MCP {content, isError}

const { getWasmHandles } = require('./turn.js');
const { requireAuth } = require('./_auth');

module.exports = async (req, res) => {
  if (!requireAuth(req, res)) return; // RT-SWEEP-2026-10-08-003: inbound auth gate
  let core;
  try {
    core = await import('../wasm/governor-wasm/shell-core.mjs');
  } catch (e) {
    res.status(500).json({ error: 'shell core failed to load' });
    return;
  }
  let h;
  try {
    h = await getWasmHandles();
  } catch (e) {
    res.status(500).json({ error: 'governor wasm failed to load: ' + String(e.message || e).slice(0, 120) });
    return;
  }

  if (req.method === 'GET') {
    const out = core.handleMcpGet(h);
    res.status(out.status).json(out.json);
    return;
  }
  if (req.method === 'POST') {
    let body = req.body;
    if (typeof body === 'string') { try { body = JSON.parse(body); } catch { body = {}; } }
    const out = core.handleMcpPost(h, body || {});
    res.status(out.status).json(out.json);
    return;
  }
  res.status(405).json({ error: 'method not allowed' });
};
