# Kokoro TTS spike — feasibility for the PUBLIC Vercel demo

Date: 2026-10-07/08. Method: real install + measurement on the dev VM
(2 vCPU, ~7.9GB RAM), plus Vercel platform limits. Measured numbers are
measured; the one latency figure I could not take locally is labeled as
prior evidence, not mine.

## Verdict: NOT shippable on Vercel serverless. Plainly.

The public demo is static files + serverless functions. A Kokoro sidecar
cannot live in either. Measured ship-weight:

| Component | Measured size |
|---|---|
| Python venv (torch 2.14.1+cpu, kokoro 0.9.4, spacy/transformers tree) | **1.5 GB** |
| Model kokoro-v1_0.pth (fp32) | **327 MB** |
| Voice pack af_heart.pt | 0.5 MB |
| espeak-ng (bundled via espeakng-loader, no apt needed) | ~15 MB inside venv |
| **Total to ship** | **~1.9 GB** |

Vercel serverless limits (hobby, the plan this demo is on): **250 MB**
unzipped bundle, **10 s** max duration, ~1 GB RAM, /tmp 512 MB ephemeral.

- Size alone kills it: 1.9 GB vs 250 MB = **7.6× over**, before any audio.
- The lean ONNX-fp16 path (~160 MB model + onnxruntime) still leaves no
  headroom once a Python runtime is included, and still dies on the 10 s
  hobby timeout at cold start (model load alone is seconds — see below).
- No persistent disk: the model must ship in the bundle; re-downloading
  327 MB per cold start inside a 10 s timeout is not a plan.

## What WAS built: local-only lane

`kokoro-lane/server.py` — stdlib HTTP sidecar (127.0.0.1:8765):
- `GET /health` → `{ok, model_loaded, voices, synthetic_only}` (lazy load)
- `POST /tts` → WAV bytes, 24 kHz mono 16-bit. JSON `{text, voice, speed}`;
  voice must be in the allowlist, speed clamped 0.5–2.0, text ≤ 2000 chars.
- Voice allowlist = Kokoro's built-in **synthetic** voices only (27 voices,
  `af_heart` default). No cloning endpoint, no training-audio pipeline —
  no biometric cloning of real persons, by construction.
- `kokoro-lane/test.sh` — starts server, health-checks, synthesizes one
  utterance, validates the WAV, reports timing.

**Validated live:** server boots, `/health` returns the voice list,
unknown voice rejected with `synthetic built-ins only`.
**Not validated end-to-end on this box:** actual synthesis. The dev VM had
~431 MB RAM free during the spike; torch + 327 MB model needs ~1 GB+ and
the loader was OOM-killed twice with no traceback. No swap permitted in
this container. On a box with 2 GB+ free it will run — the code path is
straight `KPipeline` usage.

## Latency (prior evidence, not measured this spike)

nv-war-002 (war-room, same VM class): **~120–180 ms per 1 s of utterance**
CPU-only. Consistent with Kokoro-82M's published real-time factor on CPU.
Treat as the planning figure until someone re-measures on a bigger box.

## Honest public paths (not built, named for the next spike)

1. **Sidecar on a persistent host** (his server / $0 VM / bigger dev box)
   with `/api/tts` proxying to it. Works, but the "public" demo then
   depends on a private box — fine for dogfood, fragile for the stage.
2. **In-browser ONNX** (kokoro.js / transformers.js WASM port): model
   downloads to the visitor's browser once (~160 MB fp16, cached),
   synthesis on-device. This is the real public path — no server at all.
3. **Status quo:** browser speechSynthesis + `/api/tts` proxy fallback.
   Zero infra; the "cheap demo" tell remains.

## Environment quirks found (for whoever reproduces this)

- This VM's `no_proxy` contains bare IPv6 entries (`::1`,
  `fd8b:4f84:7d32:99::1`) which crash httpx's URL parser
  (`Invalid port: ':1]'`). Workaround: sanitize `no_proxy` to
  `localhost,127.0.0.1` for HF downloads.
- `huggingface.co` HEAD requests time out through the egress proxy; the
  model/voice/config files download fine with `curl -L` (follows the
  redirect to the CDN). The HF cache can be seeded manually:
  `blobs/<sha256>` + `refs/main` + `snapshots/<commit>/` symlinks, then
  `HF_HUB_OFFLINE=1`.
- `KModel(repo_id, config=<path>, model=<path>)` + `KPipeline(..., model=km)`
  loads fully offline — no hub needed at runtime.

## Recommendation

Ship the sidecar as the dogfood/local lane (done here). Do NOT promise
neural TTS on the public Vercel URL until path 2 (in-browser ONNX) is
spiked. speechSynthesis stays the public default.
