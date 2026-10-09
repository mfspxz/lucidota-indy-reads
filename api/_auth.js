// api/_auth.js — shared inbound auth gate for all indy-reads API handlers.
//
// RT-SWEEP-2026-10-08-003 (falsification seq 131, HIGH): the API had ZERO
// inbound auth — all handlers checked POST-only and nothing else, so anyone
// reachable could burn the router key quota. Privacy-by-default violation.
//
// Every handler MUST call requireAuth(req, res) as its first statement:
//   const { requireAuth } = require('./_auth');
//   module.exports = async (req, res) => {
//     if (!requireAuth(req, res)) return;
//     ...
//   };
// requireAuth returns true when authorized; on failure it sends the denial
// response itself (401, or 500 when no key material is configured) and
// returns false.
//
// Key material: the router key the handlers already spend — bundled
// api/.router_key file FIRST (vercel.json includeFiles pulls it into the
// bundle; the dashboard env holds a stale key, same precedence as
// turn.js's bundledRouterKey), then process.env.ROUTER_KEY. No new secrets,
// no rotation (secret freeze stands).
//
// Accepted carriers (first match wins): `x-api-key` header,
// `Authorization: Bearer <key>`, `?key=` query param (?key= is needed for
// <audio>/TTS GETs that cannot set headers; it is also Zack's standing
// interim gate pattern).
//
// Fail-closed: if no key material is configured, ALL requests are denied
// with 500 — never open. Comparison is timing-safe. No dev backdoors.

const crypto = require('crypto');

let _expected; // lazy per-instance cache; undefined = not yet resolved
function expectedKey() {
  if (_expected !== undefined) return _expected;
  // Bundled file wins (stale dashboard env), then env.
  try {
    const fs = require('fs');
    const path = require('path');
    const cands = [path.join(__dirname, '.router_key'), path.join(process.cwd(), 'api', '.router_key')];
    for (const p of cands) {
      try {
        const k = fs.readFileSync(p, 'utf8').trim();
        if (k) { _expected = k; return _expected; }
      } catch {}
    }
  } catch {}
  const env = (process.env.ROUTER_KEY || '').trim();
  _expected = env || null;
  return _expected;
}

function presentedKey(req) {
  const headers = req.headers || {};
  const h = headers['x-api-key'] || headers['X-Api-Key'];
  if (typeof h === 'string' && h.trim()) return h.trim();
  const auth = headers['authorization'] || headers['Authorization'];
  if (typeof auth === 'string') {
    const m = /^Bearer\s+(\S+)\s*$/i.exec(auth.trim());
    if (m) return m[1];
  }
  const q = req.query && req.query.key;
  if (typeof q === 'string' && q.trim()) return q.trim();
  return null;
}

function safeEqual(a, b) {
  const ab = Buffer.from(a, 'utf8');
  const bb = Buffer.from(b, 'utf8');
  if (ab.length !== bb.length) return false;
  return crypto.timingSafeEqual(ab, bb);
}

function requireAuth(req, res) {
  const expected = expectedKey();
  if (!expected) {
    res.status(500).json({ error: 'auth not configured' });
    return false;
  }
  const got = presentedKey(req);
  if (got && safeEqual(got, expected)) return true;
  res.status(401).json({ error: 'unauthorized' });
  return false;
}

module.exports = { requireAuth, expectedKey, presentedKey };
