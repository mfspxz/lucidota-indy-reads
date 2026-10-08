// Health probe for the Indy_Reads demo backend — Cloudflare Pages Function (ESM).
// The UI's LIVE badge reads this: green only if the governor answers.
// router_key: boolean only — never the value. On Cloudflare the key comes
// from a Pages secret (env.ROUTER_KEY); no bundled-file logic.
function json(data, status) {
  return new Response(JSON.stringify(data), {
    status: status || 200,
    headers: { 'content-type': 'application/json' },
  });
}

export async function onRequest(context) {
  const { env } = context;
  return json({
    service: 'indy-reads', status: 'ok', mode: 'fixture',
    router_key: !!(env.ROUTER_KEY && String(env.ROUTER_KEY).trim()),
    ts: Date.now(),
  });
}
