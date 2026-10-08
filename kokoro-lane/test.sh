#!/usr/bin/env bash
# Kokoro sidecar smoke test. Starts the server, hits /health, synthesizes one
# utterance, validates the WAV, reports timing, shuts down.
# Run: ./test.sh   (from ~/workspace/indy-reads/kokoro-lane)
set -u
cd "$(dirname "$0")"
PY=.venv/bin/python
if [ ! -x "$PY" ]; then echo "no venv: run the pip install first"; exit 1; fi

KOKORO_PORT=8765 KOKORO_EAGER=1 "$PY" server.py > /tmp/kokoro-server.log 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT

echo "waiting for model load..."
for i in $(seq 1 40); do
  if curl -sf -m 2 http://127.0.0.1:8765/health | grep -q '"model_loaded": true'; then break; fi
  sleep 5
done
curl -s -m 5 http://127.0.0.1:8765/health | head -c 300; echo

TEXT="The record is thin, so the verdict stays unknown. That is the whole job."
echo "synthesizing ${#TEXT} chars..."
curl -s -m 120 -o /tmp/kokoro-out.wav -D /tmp/kokoro-hdrs.txt \
  -H 'Content-Type: application/json' \
  -d "{\"text\": $(printf '%s' "$TEXT" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'), \"voice\": \"af_heart\"}" \
  http://127.0.0.1:8765/tts
echo "--- response headers ---"
grep -i '^X-\|^HTTP\|content-type\|content-length' /tmp/kokoro-hdrs.txt
echo "--- wav validation ---"
python3 - << 'EOF'
import wave, struct
try:
    w = wave.open('/tmp/kokoro-out.wav')
    n = w.getnframes(); sr = w.getframerate()
    print(f"VALID WAV: {n} frames @ {sr}Hz = {n/sr:.2f}s audio, {w.getnchannels()}ch")
except Exception as e:
    print("INVALID WAV:", e)
EOF
ls -la /tmp/kokoro-out.wav
echo "--- server log tail ---"
tail -5 /tmp/kokoro-server.log
echo TEST-DONE
