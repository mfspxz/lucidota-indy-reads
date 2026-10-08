#!/usr/bin/env python3
"""Kokoro TTS sidecar — LOCAL-ONLY lane for Indy_Reads.

Serves Kokoro-82M neural TTS over HTTP on this machine. This is NOT shippable
on Vercel serverless (see KOKORO-SPIKE.md for the measured numbers); it exists
so the voice page can point at a real neural voice during local dev/dogfood
via a ?tts= override, with browser speechSynthesis remaining the public fallback.

Synthetic voices ONLY. Kokoro ships built-in synthetic voice packs (af_*, bf_*,
am_*, bm_*); there is no cloning endpoint here and no training-audio pipeline.
No biometric cloning of real persons, by construction: the allowlist below is
the entire voice surface.

Endpoints:
  GET  /health          -> {ok, model_loaded, voices, device}
  POST /tts             -> WAV bytes. JSON body: {text, voice, speed}
                           voice must be in VOICES (default af_heart),
                           speed 0.5..2.0 (default 1.0), text <= 2000 chars.

Env: KOKORO_PORT (default 8765), KOKORO_EAGER=1 to load the model at startup.
"""
import io
import json
import os
import time
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(os.environ.get("KOKORO_PORT", "8765"))

# Built-in synthetic Kokoro voices only. No custom/cloned voices accepted.
VOICES = [
    "af_heart", "af_bella", "af_nicole", "af_aoede", "af_kore", "af_sarah",
    "af_nova", "af_sky", "af_jessica", "af_river",
    "bf_emma", "bf_isabella", "bf_alice", "bf_lily",
    "am_adam", "am_echo", "am_eric", "am_fenrir", "am_liam", "am_michael",
    "am_onyx", "am_puck", "am_santa",
    "bm_daniel", "bm_fable", "bm_george", "bm_lewis",
]

_pipe = None
_load_s = None


def get_pipe():
    global _pipe, _load_s
    if _pipe is None:
        from kokoro import KPipeline
        t0 = time.time()
        _pipe = KPipeline(lang_code="a")  # American English
        _load_s = round(time.time() - t0, 2)
    return _pipe


def synth_wav(text, voice, speed):
    pipe = get_pipe()
    chunks = []
    for _, _, audio in pipe(text, voice=voice, speed=speed):
        chunks.append(audio)
    import torch
    full = torch.cat(chunks, dim=0) if len(chunks) > 1 else chunks[0]
    pcm = (full.clamp(-1, 1).mul(32767)).short().cpu().numpy()
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(24000)
        w.writeframes(pcm.tobytes())
    return buf.getvalue()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _json(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/health":
            self._json(200, {
                "ok": True,
                "model": "kokoro-82m",
                "model_loaded": _pipe is not None,
                "load_s": _load_s,
                "voices": VOICES,
                "synthetic_only": True,
            })
        else:
            self._json(404, {"error": "not found"})

    def do_POST(self):
        if self.path != "/tts":
            self._json(404, {"error": "not found"})
            return
        try:
            n = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(n) or b"{}")
        except Exception:
            self._json(400, {"error": "bad json"})
            return
        text = str(body.get("text", "")).strip()
        voice = str(body.get("voice", "af_heart"))
        try:
            speed = float(body.get("speed", 1.0))
        except (TypeError, ValueError):
            speed = 1.0
        if not text:
            self._json(400, {"error": "empty text"})
            return
        if len(text) > 2000:
            self._json(400, {"error": "text too long (max 2000 chars)"})
            return
        if voice not in VOICES:
            self._json(400, {"error": "unknown voice; synthetic built-ins only", "voices": VOICES})
            return
        speed = min(2.0, max(0.5, speed))
        try:
            t0 = time.time()
            wav = synth_wav(text, voice, speed)
            synth_s = round(time.time() - t0, 3)
        except Exception as e:
            self._json(500, {"error": "synthesis failed", "detail": str(e)[:200]})
            return
        self.send_response(200)
        self.send_header("Content-Type", "audio/wav")
        self.send_header("Content-Length", str(len(wav)))
        self.send_header("X-Synth-Seconds", str(synth_s))
        self.send_header("X-Voice", voice)
        self.end_headers()
        self.wfile.write(wav)


if __name__ == "__main__":
    if os.environ.get("KOKORO_EAGER") == "1":
        print("loading kokoro-82m ...", flush=True)
        get_pipe()
        print(f"loaded in {_load_s}s", flush=True)
    srv = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    print(f"kokoro sidecar on 127.0.0.1:{PORT} (local-only, synthetic voices)", flush=True)
    srv.serve_forever()
