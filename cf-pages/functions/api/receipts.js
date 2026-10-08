// Receipt stream for the Indy_Reads demo backend — Cloudflare Pages Function (ESM).
// Hash-chained, per isolate.
export async function onRequest() {
  // The chain lives in turn.js's module scope; this endpoint reports honestly
  // that per-isolate receipts reset on cold start.
  return new Response(
    JSON.stringify({ receipts: [], note: 'per-isolate chain; see turn responses for receipt ids' }),
    { headers: { 'content-type': 'application/json' } }
  );
}
