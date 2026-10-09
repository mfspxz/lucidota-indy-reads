// TTS proxy: server-side fetch of translate_tts, returned as audio/mpeg.
// The browser calls same-origin /api/tts?q=... so no referer/CORS blocking.
const { requireAuth } = require('./_auth');
module.exports = async (req, res) => {
  if (!requireAuth(req, res)) return; // RT-SWEEP-2026-10-08-003: inbound auth gate
  try {
    const q = String((req.query && req.query.q) || '').slice(0, 200);
    if (!q) { res.status(400).send('missing q'); return; }
    const url = 'https://translate.google.com/translate_tts?ie=UTF-8&tl=en&client=tw-ob&q=' + encodeURIComponent(q);
    const r = await fetch(url, { headers: { 'User-Agent': 'Mozilla/5.0 (Linux; Android 10)' } });
    if (!r.ok) { res.status(502).send('tts upstream ' + r.status); return; }
    const buf = Buffer.from(await r.arrayBuffer());
    if (buf.length < 100) { res.status(502).send('tts empty'); return; }
    res.setHeader('Content-Type', 'audio/mpeg');
    res.setHeader('Cache-Control', 'public, max-age=86400');
    res.send(buf);
  } catch (e) {
    res.status(500).send('tts error');
  }
};
