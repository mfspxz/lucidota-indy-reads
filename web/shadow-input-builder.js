// shadow-input-builder.js — single source of truth for the WASM governor
// shadow-mode input construction.
//
// The shadow path replicates EXACTLY what the server builds for a
// conversational turn (shell-core.mjs converse branch) and sends it to the
// client-side Rust governor (WASM) as an independent second opinion.
// Shadow mode NEVER overrides the server verdict — agreement/disagreement
// is only logged.
//
// Consumed by BOTH:
//   - web/voice.js::wasmShadowCheck (browser, classic <script>)
//   - wasm/governor-wasm/test/shadow-agreement.mjs (node, ESM import via
//     globalThis — same pattern as web/governor-wasm-loader.js)
//
// Previously the differential harness hand-mirrored this shape in a
// SHADOW_PROPOSAL constant; if the two ever drifted, the differential
// would silently validate a fiction. This module makes drift structurally
// impossible: there is exactly one copy of the construction logic.
//
// No dependencies. Pure functions; safe to call any number of times.
(function () {
  'use strict';

  // The complete, honest proposal shape the server builds for a
  // conversational turn. decide() derives epistemic_status from
  // (claim_text, evidence) only — claim:null falls back to the utterance,
  // evidence is [] client and server side — so this tracks the server
  // verdict. Returns a FRESH object every call (no shared mutation).
  function buildShadowProposal() {
    return {
      intent: 'converse',
      claim: null,
      hypothesis: null,
      requested_action: null,
      response_style: 'conversational',
    };
  }

  // Build the exact input JSON string the shadow path sends to verdict().
  // Key order is load-bearing for byte-fidelity: request_id, session_id,
  // utterance, proposal, evidence.
  function buildShadowInput(utterance, requestId, sessionId) {
    return JSON.stringify({
      request_id: requestId,
      session_id: sessionId,
      utterance: utterance,
      proposal: buildShadowProposal(),
      evidence: [],
    });
  }

  var api = {
    buildShadowProposal: buildShadowProposal,
    buildShadowInput: buildShadowInput,
  };
  if (typeof window !== 'undefined') window.ShadowInputBuilder = api;
  else if (typeof globalThis !== 'undefined') globalThis.ShadowInputBuilder = api;
})();
