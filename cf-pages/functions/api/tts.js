// TTS proxy — Cloudflare Pages Function (ESM).
// Server-side fetch of translate_tts, returned as audio/mpeg.
// The browser calls same-origin /api/tts?q=... so no referer/CORS blocking.
export async function onRequest(context) {
  const { request } = context;
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
