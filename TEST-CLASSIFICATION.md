# TEST CLASSIFICATION — indy-reads (R-7 rollout, 2026-10-10)

Tags: **PROPERTY** = invariant stated without implementation terms.
**CHANGE-DETECTION** = pins current behavior; update freely.
**SMOKE** = operational integration smoke test (outside the
PROPERTY/CHANGE-DETECTION binary; asserts the thing runs, not an
invariant).

No test files were modified this round (classification only).

## prosody-test.js (node harness, ~20 assertions)

| assertion | class |
|---|---|
| wardrobe yields exactly 60 combos | CHANGE-DETECTION (exact count pin) |
| strict rookie>prime>veteran ordering on every axis | PROPERTY (era-ordering design invariant) |
| all attrs within clamp bounds | PROPERTY (safety-bounds invariant) |
| forgiving parse: garbage era/tone/position -> prime/dry/middle | PROPERTY (fail-safe defaults) |
| defaults with no args | CHANGE-DETECTION (default-value pin) |
| attrs deterministic (same input, same output) | PROPERTY |
| emphasis deterministic and tone-independent | PROPERTY (emphasis rule) |
| emphasisWord longest-word rule + examples | MIXED (rule = PROPERTY; exact examples = CHANGE-DETECTION) |
| empty text -> empty emphasis | CHANGE-DETECTION (edge pin) |
| morphPlan: segments == sentences | PROPERTY (morph schedule invariant) |
| era walks rookie->prime->veteran; position labels | PROPERTY |
| morphPlan deterministic | PROPERTY |
| no-morph single segment; empty text -> no segments; reverse morph | MIXED (schedule behavior pins) |
| long sentence clause-split (>140 chars) | CHANGE-DETECTION (threshold pin) |
| attrs shape matches what voice.js speak() consumes | CHANGE-DETECTION (consumer-contract pin) |
| header 60-combo table matches wardrobe() output exactly | CHANGE-DETECTION (doc-honesty pin, explicitly labeled as such) |

Rough: 8 PROPERTY / 6 CHANGE-DETECTION / 2 MIXED.

## kokoro-lane/test.sh

SMOKE: starts the Kokoro sidecar, polls /health, synthesizes one
utterance, validates the WAV decodes, reports timing, shuts down. The
WAV-validity check is the closest to a property ("synthesis returns
decodable audio"); the rest is operational wiring. Not a security
suite; no PROPERTY/CHANGE-DETECTION claims made.
