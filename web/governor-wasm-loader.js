// governor-wasm-loader.js — browser loader for governor_wasm.wasm.
//
// Wave-24: loads the wasm module built from the REAL Rust governor sources
// (wasm/governor-wasm). Hand-rolled wasm-bindgen string convention
// (retptr -> [out_ptr, out_len]), same as the node loader.
//
// Exposes: window.GovernorWASM = {
//   ready: Promise<boolean>,   // resolves true when the module is live
//   verdict(inputJson: string) // -> GovernorDecision JSON (shadow mode)
// }
// voice.js (shadow mode) calls verdict() and reads v.epistemic_status,
// v.authority_rejected, v.reason. authority_rejected is derived from the
// verdict reason — the Rust decision carries no such flag (the verdict
// reason IS the authority record).
//
// Shadow mode: the module re-verdicts each reply client-side as an
// INDEPENDENT second opinion. It NEVER overrides the server verdict.
// If the module fails to load, the page works exactly as before.

(function () {
  'use strict';

  var readyResolve;
  var ready = new Promise(function (res) { readyResolve = res; });
  var gov = null; // { decide(inputJson) -> object }

  function fail() { try { readyResolve(false); } catch (e) {} }

  function boot(bytes) {
    var memoryU8, memoryU32;
    function readStr(ptr, len) {
      return new TextDecoder().decode(memoryU8().subarray(ptr, ptr + len));
    }
    var imports = {
      env: { indy_host_now_ms: function () { return Date.now(); } },
      __wbindgen_placeholder__: {
        __wbindgen_describe: function () {},
        __wbg___wbindgen_throw_41e9ee4f547fc59a: function (ptr, len) {
          throw new Error('governor wasm: ' + readStr(ptr, len));
        },
      },
      __wbindgen_externref_xform__: {
        __wbindgen_externref_table_set_null: function () {},
        __wbindgen_externref_table_grow: function () { return 0; },
      },
    };
    return WebAssembly.instantiate(bytes, imports).then(function (mod) {
      var ex = mod.instance.exports;
      var names = Object.keys(ex);
      memoryU8 = function () { return new Uint8Array(ex.memory.buffer); };
      memoryU32 = function () { return new Uint32Array(ex.memory.buffer); };
      var malloc = ex.__wbindgen_malloc, free = ex.__wbindgen_free;
      if (typeof malloc !== 'function' || typeof free !== 'function' || !ex.memory) {
        throw new Error('governor wasm: missing malloc/free/memory');
      }
      function findFn(prefix) {
        for (var i = 0; i < names.length; i++) {
          var n = names[i];
          if ((n === prefix || n.indexOf(prefix + '_') === 0) && typeof ex[n] === 'function') return ex[n];
        }
        return null;
      }
      // f(retptr, ptr, len) -> JSON string
      function callStr(fn, input) {
        var data = new TextEncoder().encode(input);
        var ptr = malloc(data.length, 1);
        memoryU8().set(data, ptr);
        var retptr = malloc(8, 1);
        try {
          fn(retptr, ptr, data.length);
          var u32 = memoryU32();
          var outPtr = u32[retptr / 4], outLen = u32[retptr / 4 + 1];
          var out = readStr(outPtr, outLen);
          free(outPtr, outLen, 1);
          return out;
        } finally {
          free(retptr, 8, 1);
        }
      }
      var shimNew = findFn('wasmgovernor_new');
      var shimFree = findFn('__wbg_wasmgovernor_free');
      var shimDecide = findFn('wasmgovernor_decide');
      if (!shimNew || !shimFree || !shimDecide) {
        throw new Error('governor wasm: missing governor shims');
      }
      // f(retptr, self_ptr, ptr, len) -> JSON string
      function callSelf(fn, selfPtr, input) {
        var data = new TextEncoder().encode(input);
        var ptr = malloc(data.length, 1);
        memoryU8().set(data, ptr);
        var retptr = malloc(8, 1);
        try {
          fn(retptr, selfPtr, ptr, data.length);
          var u32 = memoryU32();
          var outPtr = u32[retptr / 4], outLen = u32[retptr / 4 + 1];
          var out = readStr(outPtr, outLen);
          free(outPtr, outLen, 1);
          return out;
        } finally {
          free(retptr, 8, 1);
        }
      }
      var selfPtr = shimNew();
      gov = {
        decide: function (input) { return JSON.parse(callSelf(shimDecide, selfPtr, JSON.stringify(input))); },
      };
      readyResolve(true);
    }).catch(function (e) {
      try { console.warn('[wasm-governor] load failed:', e); } catch (x) {}
      fail();
    });
  }

  // verdict(inputJson): shadow-mode entry. Accepts the voice.js shape
  // {request_id, session_id, utterance, proposal, evidence} and runs the
  // REAL Rust decide pipeline on EXACTLY the proposal the caller sent —
  // the adapter never substitutes or rewrites it (wave-33: the old code
  // silently replaced any proposal with a fixed converse one, so what
  // voice.js sent was not what ran). A missing/non-object proposal is a
  // fail-closed error, never a silent default: shadow must not invent
  // inputs. Returns the GovernorDecision JSON plus the derived
  // authority_rejected flag voice.js reads.
  function verdict(inputJson) {
    if (!gov) return { error: 'wasm governor not loaded' };
    var input;
    try { input = JSON.parse(inputJson); }
    catch (e) { return { error: 'bad input json' }; }
    var utterance = String(input.utterance || '');
    var proposal = input.proposal;
    if (!proposal || typeof proposal !== 'object' || Array.isArray(proposal)) {
      return { error: 'bad proposal shape: shadow refuses to invent inputs' };
    }
    var d;
    try {
      d = gov.decide({
        request_id: String(input.request_id || 'w-' + Date.now().toString(36)),
        session_id: String(input.session_id || 's-wasm'),
        utterance: utterance,
        proposal: proposal,
        evidence: Array.isArray(input.evidence) ? input.evidence : [],
      });
    } catch (e) {
      return { error: 'governor throw: ' + String((e && e.message) || e).slice(0, 80) };
    }
    if (d && d.error) return { error: d.error };
    d.authority_rejected = /authority REJECTED/.test(d.reason || '');
    return d;
  }

  var api = { ready: ready, verdict: verdict };
  if (typeof window !== 'undefined') window.GovernorWASM = api;
  else if (typeof globalThis !== 'undefined') globalThis.GovernorWASM = api;

  // governor_wasm.wasm.gz sits next to this loader; gunzip in-page.
  fetch('governor_wasm.wasm.gz')
    .then(function (r) {
      if (!r.ok) throw new Error('http ' + r.status);
      return r.arrayBuffer();
    })
    .then(function (buf) {
      if (typeof DecompressionStream !== 'undefined') {
        var ds = new DecompressionStream('gzip');
        var stream = new Blob([buf]).stream().pipeThrough(ds);
        return new Response(stream).arrayBuffer();
      }
      return buf; // already plain wasm
    })
    .then(function (buf) { boot(new Uint8Array(buf)); })
    .catch(function (e) {
      try { console.warn('[wasm-governor] fetch failed:', e); } catch (x) {}
      fail();
    });
})();
