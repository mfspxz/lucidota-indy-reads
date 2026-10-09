// api/receipts.js — receipt stream for the Indy_Reads demo backend. THIN SHELL.
//
// Wave-24: serves the wasm governor's hash-chained receipt stream
// (per function instance, as before — warm instances accumulate turns).
// No receipt logic in JS; the chain lives in Rust.
const { getWasmHandles } = require('./turn.js');

module.exports = async (req, res) => {
  let h;
  try {
    h = await getWasmHandles();
  } catch (e) {
    res.status(500).json({ error: 'governor wasm failed to load' });
    return;
  }
  try {
    const n = Math.min(200, Math.max(1, parseInt((req.query && req.query.n) || '50', 10) || 50));
    const receipts = h.gov.recent_receipts(n);
    if (receipts && receipts.error) throw new Error(receipts.error);
    res.json({ receipts, note: 'per-instance chain; resets on cold start' });
  } catch (e) {
    h.resetGov();
    res.status(500).json({ error: 'governor fault: ' + String((e && e.message) || e).slice(0, 120) });
  }
};
