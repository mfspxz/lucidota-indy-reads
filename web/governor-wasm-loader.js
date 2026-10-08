// governor-wasm-loader.js — hand-rolled browser loader for governor_wasm.wasm.
//
// The Rust governor (wasm/governor-wasm, pure deterministic logic, no
// axum/tokio) compiled to wasm32-unknown-unknown. No bundler, no
// wasm-bindgen CLI: this implements the wasm-bindgen string convention by
// hand — f(retptr, ptr, len) with retptr -> [out_ptr:u32, out_len:u32],
// allocation via the exported __wbindgen_malloc/__wbindgen_free.
//
// Loader quirks (from the spike, /tmp/gw-load.cjs):
//   - export names may be mangled: resolve by prefix match
//     (exact name OR name starting with prefix + "_")
//   - __wbindgen_malloc takes (size, align)
//   - do NOT free input pointers (only out + retptr)
//   - stub __wbindgen_describe as a no-op
//   - the module's own table is not exported: give the externref-xform
//     imports a private table (pure string functions never touch it)
//
// Exposes: window.GovernorWASM = {
//   ready: Promise<boolean>,          // false on any failure — page works as before
//   verdict(inputJson: string) -> object|null,   // throws if not ready
//   normalize(s: string) -> string|null,         // throws if not ready
// }
// Shadow mode only: the page never lets this override the server verdict.
(function () {
  'use strict';
  var api = {
    ready: null,
    verdict: function () { throw new Error('governor wasm not ready'); },
    normalize: function () { throw new Error('governor wasm not ready'); },
  };
  if (typeof window !== 'undefined') window.GovernorWASM = api;
  else if (typeof globalThis !== 'undefined') globalThis.GovernorWASM = api;

  function resolveByPrefix(names, prefix) {
    for (var i = 0; i < names.length; i++) {
      var n = names[i];
      if (n === prefix || n.indexOf(prefix + '_') === 0) return n;
    }
    return null;
  }

  api.ready = (async function () {
    try {
      // Served gzipped (the 157KB raw module blows the deploy tool's
      // 128KB single-argument limit); gunzip in-browser.
      var resp = await fetch('governor_wasm.wasm.gz');
      if (!resp || !resp.ok || !resp.body) return false;
      var bytes = await new Response(
        resp.body.pipeThrough(new DecompressionStream('gzip'))
      ).arrayBuffer();
      if (!bytes || bytes.byteLength < 1000) return false;
      var extTable;
      try {
        extTable = new WebAssembly.Table({ initial: 4, element: 'externref' });
      } catch (e) { return false; }
      var imports = {
        __wbindgen_placeholder__: {
          __wbindgen_describe: function () { /* descriptor protocol; unused */ },
        },
        __wbindgen_externref_xform__: {
          __wbindgen_externref_table_grow: function (d) { return extTable.grow(d); },
          __wbindgen_externref_table_set_null: function (idx) { extTable.set(idx, undefined); },
        },
      };
      var inst = await WebAssembly.instantiate(bytes, imports);
      var ex = inst.instance.exports;
      var names = Object.keys(ex);
      var mallocName = resolveByPrefix(names, '__wbindgen_malloc');
      var freeName = resolveByPrefix(names, '__wbindgen_free');
      if (!mallocName || !freeName || !ex.memory) return false;
      var malloc = ex[mallocName], free = ex[freeName];
      var memU8 = function () { return new Uint8Array(ex.memory.buffer); };
      var memU32 = function () { return new Uint32Array(ex.memory.buffer); };
      var readStr = function (ptr, len) {
        return new TextDecoder().decode(memU8().subarray(ptr, ptr + len));
      };
      var findFn = function (prefix) {
        var n = resolveByPrefix(names, prefix);
        return n && typeof ex[n] === 'function' ? ex[n] : null;
      };
      var verdictFn = findFn('verdict');
      var normalizeFn = findFn('normalize');
      if (!verdictFn || !normalizeFn) return false;

      // Smoke test: normalize must round-trip before we trust the module.
      var smoke;
      try {
        smoke = (function () {
          var data = new TextEncoder().encode('smoke');
          var ptr = malloc(data.length, 1);
          memU8().set(data, ptr);
          var retptr = malloc(8, 1);
          normalizeFn(retptr, ptr, data.length);
          var u32 = memU32();
          var out = readStr(u32[retptr / 4], u32[retptr / 4 + 1]);
          free(u32[retptr / 4], u32[retptr / 4 + 1], 1);
          free(retptr, 8, 1);
          return out;
        })();
      } catch (e) { return false; }
      if (smoke !== 'smoke') return false;

      function callStr(fn, input) {
        var data = new TextEncoder().encode(input);
        var ptr = malloc(data.length, 1);
        memU8().set(data, ptr);
        var retptr = malloc(8, 1);
        fn(retptr, ptr, data.length);
        var u32 = memU32();
        var outPtr = u32[retptr / 4], outLen = u32[retptr / 4 + 1];
        var out = readStr(outPtr, outLen);
        free(outPtr, outLen, 1);
        free(retptr, 8, 1);
        // NOTE: input pointer intentionally NOT freed (spike quirk).
        return out;
      }
      api.verdict = function (inputJson) {
        return JSON.parse(callStr(verdictFn, inputJson));
      };
      api.normalize = function (s) {
        return callStr(normalizeFn, s);
      };
      return true;
    } catch (e) {
      return false;
    }
  })();
})();
