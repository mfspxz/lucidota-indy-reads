# SPREE-REDTEAM-A2 — Indy_Reads API Red Team

**Agent:** A2 (red team) · **Date:** 2026-10-09 ~02:00 PDT
**Target:** `https://indy-reads-ui-maroonedpilot-8906.vercel.app` (production, post-auth-deploy dpl_4W3knRJam34tL6sGFnddi3JwuknX)
**Method:** Unauthenticated black-box probing (live) + source review of `api/_auth.js`, `turn.js`, `tts.js`, `mcp.js`, `receipts.js`, `health.js`, `web/voice.js`
**Constraint:** No real key used at any point. Bypass only — authorized-path testing is out of scope.

## Verdict: NO AUTH BYPASS FOUND — the gate holds

All 5 handlers (`turn`, `health`, `receipts`, `tts`, `mcp`) call `requireAuth(req, res)` as their first statement (source-verified, lines confirmed). Live probing confirms.

## Bypass battery — 22 attempts, all denied

| # | Attempt | Result |
|---|---------|--------|
| 1 | `GET /api/health` (no creds) | 401 |
| 2 | `POST /api/turn` (no creds) | 401 |
| 3 | `GET /api/mcp` (no creds) | 401 |
| 4 | `POST /api/mcp` (no creds) | 401 |
| 5 | `GET /api/tts?q=hi` (no creds) | 401 — gate runs BEFORE param validation |
| 6 | `GET /api/receipts` (no creds) | 401 |
| 7 | `Authorization: Bearer` (no token) | 401 |
| 8 | `Authorization: Bearer    ` (spaces) | 401 |
| 9 | `authorization: bearer deadbeef` (lowercase) | 401 |
| 10 | `Authorization: BEARER deadbeef` (upper) | 401 |
| 11 | `x-api-key:` (empty) | 401 |
| 12 | `x-api-key: wrong` | 401 |
| 13 | `?key=` (empty) | 401 |
| 14 | `?key=wrong` | 401 |
| 15 | `?key=a&key=b` (array) | 401 — `typeof` guard rejects arrays |
| 16 | `?key=0` | 401 |
| 17 | 64-hex wrong key (correct length) | 401 |
| 18 | `HEAD /api/health` | 401 |
| 19 | `OPTIONS /api/health` | 401 |
| 20 | `/api/health/` (trailing slash) | 401 |
| 21 | `/api//health` (double slash) | 308 → canonical `/api/health` → 401 |
| 22 | `/API/health` (case) | 404 |

Error-body oracle check: missing-key and wrong-key return byte-identical `{"error":"unauthorized"}`. **No oracle. PROVEN.**
CORS: no `Access-Control-Allow-*` headers on 401s. No wildcard. **PROVEN.**

## Findings

### MEDIUM — Bearer key routinely transmitted in URLs (`?key=`)
`web/voice.js:24` appends `?key=<secret>` to every API call. Consequences:
- The demo link IS the key: browser history, address bar (shoulder-surf), shareable links, Vercel access logs all carry it.
- `api/tts.js` serves audio with `Cache-Control: public, max-age=86400` — the secret is embedded in edge cache keys (keyed by full URL).
- **Blast radius:** per `_auth.js` docs, the inbound key IS the outbound router key (same material). A leaked demo link = direct `ai-router` quota burn — the attacker doesn't even need the demo, they can hit the router directly.
- This is Zack's standing interim `?key=` pattern (documented tradeoff for `<audio>` GETs that can't set headers), but `fetch()` calls have no such constraint.
- **Recommend:** use `Authorization` header for `fetch()`; reserve `?key=` for the `<audio>` element only. Longer-term: separate inbound demo key from router key, or short-lived per-session tokens.

### LOW — `safeEqual` length-check early return (theoretical timing oracle)
`api/_auth.js`: `if (ab.length !== bb.length) return false` runs BEFORE `crypto.timingSafeEqual`. An attacker who can measure response time precisely could binary-search the key length.
**Measured:** 20 samples each — 3-char wrong key avg 1585ms vs 64-hex wrong key avg 1430ms. No signal; Vercel cold-start variance (hundreds of ms) swamps the nanosecond-scale difference. **NOT practically exploitable. PROVEN by measurement.**
Fix (one line): compare `sha256(got)` vs `sha256(expected)` — fixed-length, no length oracle.

### LOW — No rate limiting on the auth endpoint
Unlimited 401 probing is possible. At 256-bit key entropy brute force is infeasible, so this is recon-noise only. Informational. (Vercel has platform-level abuse controls.)

### INFO — Good: no key leakage in error paths
`turn.js` error/catch paths never interpolate key material (source-grepped). The bundled `.router_key` is pulled into the function bundle via `vercel.json` `includeFiles` but never served (no route exposes it; `outputDirectory` is `web/`).

## Attack surface reviewed but not reachable unauthenticated
- **MCP `handleMcpPost`** (wasm): tool/argument injection untested — gated behind auth. Would need authorized testing.
- **`turn.js` LLM path**: prompt injection untested — gated behind auth.
- **TTS upstream**: `translate.google.com/translate_tts` URL is hardcoded; `q` is `encodeURIComponent`'d and sliced to 200 chars. No SSRF. With a valid key, arbitrary-text TTS is the intended feature.
- **`?key=` Referer leakage:** `web/voice.html` loads zero third-party resources (verified — no external URLs), so no Referer exfil to third parties from the page itself.

## Recommendations (priority order)
1. **Move `fetch()` calls off `?key=`** → `Authorization: Bearer` header. Keep `?key=` only for `<audio>` (voice.js:24).
2. **Split inbound demo key from router key** — a leaked demo link currently equals router quota burn.
3. **Fix `safeEqual`** to compare fixed-length digests (kills the theoretical length oracle).
4. **Authorized-path testing** (needs key, separate task): MCP argument injection, turn.js prompt injection, TTS cache-key behavior with valid key.

## Artifacts
- This report: `~/workspace/indy-reads/SPREE-REDTEAM-A2.md` (not committed, per instructions)
- No code changes made. No commits.
