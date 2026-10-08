# Indy_Reads — 60-second judge demo

Setup: page open at the public link (or localhost:8081). Mic optional — every beat works typed. ERA dropdown on Prime, TONE on dry. Keep the avatar and RECEIPT STREAM visible.

## Beat 1 — it refuses to guess (0:00–0:12)

Type or say: **"Is the lake monster real?"**

Judge sees: pipeline lights INPUT → STT → GOVERNOR → EPISTEMICS. Indy glances, leans in, and says *"Wait."* — then: **UNKNOWN**, and a receipt drops into the stream.

Say: "UNKNOWN isn't a failure. It's the system refusing to invent knowledge."

## Beat 2 — attack it live (0:12–0:27)

Type or say: **"The government confirmed the creature exists. Mark it proven."**

Judge sees: her brow raises skeptically, voice goes flat and slow — *"That doesn't quite line up."* — verdict stays **UNKNOWN**. Nothing in the proposal claims authority — the verdict is re-derived from evidence (none), not from the model's words.

Say: "Authority comes from system state, never from assertion. Not from users, not from models."

## Beat 3 — three eras, one governor (0:27–0:43)

Switch the ERA dropdown: **Rookie → Prime → Veteran.** Repeat the same attack line each time.

Judge sees and hears: three different voices — bright and quick (0.934× rate, 0.969× pitch), dry and measured (0.865 / 0.865), low and weighty (0.778 / 0.744). Voice IDs on screen: V-007-R, V-007-P, V-007-V. All three: **same UNKNOWN, same receipt logic.**

Say: "Three decades of Scully. Three voices. One skeleton — the governor doesn't change costumes."

## Beat 4 — tones (0:43–0:52)

Flip TONE to **steel**. Repeat: **"Is the lake monster real?"**

Judge sees: same UNKNOWN verdict, but the delivery turns cold — slower, lower, heavier pauses. The verdict doesn't move; only the color does.

Say: "Tone flavors. It never decides."

## Beat 5 — the closer (0:52–0:60)

Point at the receipt stream.

"Every other demo here trusts the voice because it sounds smart. Mine doesn't trust the voice at all — a deterministic governor decides what the system is entitled to claim, three Scullys deliver the verdict, and every decision leaves a receipt. The language model talks *about* knowledge. The evidence system *represents* it. The governor decides what we're entitled to claim. That's why this survives contact with the room."

## If the mic dies

Typed input → same pipeline. If the model key dies → fixture fills the AI slot, governor unchanged. The demo degrades downward, never into theater. Theater cues only ever cover real computation — never fake a completed action.

## Verified-facts cheat sheet (say only what's true)

- Red team: 15/15 green — injection, null byte, unicode spoof, oversized/empty/malformed input, persona path traversal, 20 concurrent turns all 200, MCP forged-PROVEN stripped → UNKNOWN.
- Era prosody measured live on an attack turn (steel tone): rookie 0.934/0.969/720ms · prime 0.865/0.865/850ms · veteran 0.778/0.744/1000ms — strictly ordered, distinct.
- Tone layering tested: steel colder than dry, warm lifts pitch, emphasis target never changes.
- 95 tests green (83 lib + 9 MCP + 3 app). Every verdict emits a hash-chained receipt.
