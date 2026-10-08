# VOICE-CLONING.md — open-source cloning lane for Indy_Reads

Research brief, 2026-10-07. Verdicts: PROVEN (measured/cited), UNKNOWN otherwise.

## HARD RULE (constitutional)

**No cloning of real living persons.** This lane synthesizes consenting or
original voices only: owner-recorded reference clips, or licensed-original
material with documented consent. The rule is structural, not a policy doc:
`ConsentRecord` in `src/voice.rs` has exactly two variants
(`OwnerRecorded`, `LicensedOriginal`) — there is no enum arm that can express
"clone this real person," and `check_consent()` fails closed on empty records.

## Engine comparison (OSS, Oct 2026)

| Engine | Cloning | Licence | Commercial | Languages | Ref audio | Verdict |
|---|---|---|---|---|---|---|
| **Chatterbox** (Resemble AI) | yes | **MIT** | YES | en | 7–20 s clean clips | **PRIMARY PICK** — 24k+ stars, 1M+ HF downloads, 500k+ hrs training, best measured OSS English fidelity |
| **VoxCPM2** | yes | **Apache-2.0** | YES | 30 | 5 s–10 min; accepts 16 kHz | ALTERNATE — 48 kHz out via AudioVAE V2 super-resolution, documented fine-tune path |
| XTTS v2 (Coqui) | yes | Coqui Public Model License | **NO** | 17 | ~6 s | LEGACY — personal/research only |
| F5-TTS | yes | CC-BY-NC-4.0 | NO | en/zh | ~10 s | research lane |
| Fish Speech / OpenAudio S1-mini | yes | CC-BY-NC-SA-4.0 | NO | multilingual | short | research lane |
| Kokoro-82M | **no** (presets) | Apache-2.0 | YES | — | n/a | not a cloner; CPU fallback for preset voices |

## GPU requirements

- **This VM: no GPU** (2 CPU / 7.7 GB RAM). The lane ships as a stub
  (`NullCloneEngine` → `CloneError::NotConfigured`), fail-closed.
- Chatterbox fp16 inference: **~4–8 GB VRAM** on a consumer card (RTX 3060
  12GB is comfortable). XTTS v2 reference: 1.88 GB weights FP32, ~3–4 GB
  total with CUDA buffers.
- Production (measured by self-hosting guides, 2026): RTX 5090 32GB handles
  20–30 concurrent XTTS-class streams; L40S 48GB handles 4–6 workers at
  50–100 concurrent requests; H100 80GB for multi-tenant.
- **Cheapest viable path: one consumer-GPU host** (local box or spot cloud)
  running a sidecar HTTP service; Indy governor calls it through the
  `CloneEngine` trait. No cloud-API keys needed, audio never leaves the host.

## Reference-audio requirements (consenting voices only)

- Format: mono WAV, 16–44.1 kHz, no reverb, no background noise, no music.
- Length: **7–20 s** for Chatterbox (its measured sweet spot); 5 s–10 min
  for VoxCPM2 fine-tuning.
- Content: natural, consistent delivery in the target range; avoid acting.
  Closet-with-clothes recording is the standard free recipe.
- Validation before ingest: duration check (against
  `CloneCapabilities::ref_seconds_min/max`), clipping check, silence trim.
- Storage: reference clips are PII-adjacent — private by default, never in
  git, never on a public URL. `~/workspace/indy-reads/voices/` is gitignored.

## Architecture: how the lane plugs in

```
Governor event ──> VoiceState (rate/pitch/pause/era/tone)
                        │
                        ▼
              CloneRequest { text, voice_id, reference_wav, consent, voice }
                        │
                        ▼
              CloneEngine::synthesize  ── trait; NullCloneEngine today,
                        │                 Chatterbox sidecar tomorrow
                        ▼
              ClonedAudio { pcm16, 24kHz } ── feeds the SAME playback
                        │                      queue + avatar mark protocol
                        ▼                      as browser TTS (live.js)
              /api/alebex/call/:id mark hold/release
```

- Cloned audio enters the existing live-call pipeline as PCM16; the
  mark-hold-until-drain-echo protocol is backend-agnostic.
- Provenance: every utterance keeps `provenance: SYNTHETIC` + voice ID +
  graph revision. Cloned voices add the backend name to provenance.
- Era/tone layering (`derive_voice_full`) is orthogonal: the clone renders
  the voice, the governor still decides the prosody.

## Follow-up jobs (ordered)

1. `voices/` directory + `.gitignore` (reference clips, consent log).
2. Chatterbox sidecar: `python -m chatterbox` HTTP wrapper on a GPU host;
   `ChatterboxEngine` impl of `CloneEngine` (POST wav → PCM16).
3. Reference-audio validator (duration/clip/silence) as a Rust organelle.
4. Provenance extension: backend name in `VoiceProvenance`.
5. VoxCPM2 adapter (multilingual lane) once Chatterbox is wired.

## Live wiring (2026-10-07)

`POST /api/turn` accepts optional clone params; the governor's verdict and
reply text are untouched by this block — voice is presentation only.

```json
{ "utterance": "...", "clone": {
    "backend": "chatterbox",
    "reference_wav": "/path/to/consented-ref.wav",
    "consent": "owner:1728230400000" } }
```

Env overrides (request wins): `INDY_CLONE_BACKEND`, `INDY_CLONE_REF_WAV`,
`INDY_CLONE_CONSENT`. Consent strings: `owner:<recorded_at_ms>` or
`licensed:<licence_ref>` — nothing else parses.

Response fields: `clone_status` (`off` | `synthesized` | `fallback` |
`refused`) and `clone_audio` (`{backend, sample_rate, samples,
pcm16_base64}`) when synthesized. On `synthesized`, provenance `voice_id`
gains a `+clone` suffix (e.g. `V-007-P+clone`); `provenance: SYNTHETIC`
is unchanged.

Decision table (pure, in `voice::decide_clone`):
- nothing requested → `Off` → deterministic prosody, `clone_status: off`
- consent unparseable/empty → `Refused(ConsentMissing)`
- no reference clip → `Refused(NoReferenceClip)`
- valid plan → `attempt_clone` runs `check_consent` FIRST, then the engine;
  `NotConfigured`/`BackendUnavailable`/`SynthesisFailed` → prosody fallback,
  `clone_status: fallback`

This host (no GPU) runs `NullCloneEngine`: every valid plan resolves to
`fallback`. A GPU host swaps `clone_engine_from_env()` for a real
`CloneEngine`; `api.rs` does not change.
