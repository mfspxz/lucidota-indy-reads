// Health probe for the Indy_Reads demo backend. The UI's LIVE badge
// reads this: green only if the governor answers.
// router_key: boolean only — never the value. Diagnostic for the bundled-key path.
const { requireAuth } = require('./_auth');
function hasRouterKey() {
  if (process.env.ROUTER_KEY) return true;
  try {
    const fs = require('fs'), path = require('path');
    for (const p of [path.join(__dirname, '.router_key'),
                     path.join(process.cwd(), 'api', '.router_key')]) {
      try { if (fs.readFileSync(p, 'utf8').trim()) return true; } catch (e) {}
    }
  } catch (e) {}
  return false;
}
module.exports = async (req, res) => {
  if (!requireAuth(req, res)) return; // RT-SWEEP-2026-10-08-003: inbound auth gate
  res.json({ service: 'indy-reads', status: 'ok', mode: 'fixture',
             router_key: hasRouterKey(), ts: Date.now() });
};
