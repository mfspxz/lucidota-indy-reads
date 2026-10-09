// governor-wasm loader — node + browser.
// Loads the governor_wasm.wasm module built from the REAL Rust governor
// sources (wasm/governor-wasm, wave-24). Hand-rolled wasm-bindgen string
// convention (retptr -> [out_ptr, out_len]), same as the proven spike loader.
//
// Host imports the module needs:
//   env.indy_host_now_ms            -> clock (Date.now())
//   __wbindgen_placeholder__.*      -> wasm-bindgen shims (describe no-op,
//                                      throw rethrows as JS Error)
//   __wbindgen_externref_xform__.*  -> externref table shims (unused by the
//                                      string-only API, stubbed)
//
// Exposes:
//   loadGovernor(wasmBytes: Uint8Array|Buffer) -> Promise<{
//     classify_intent(utterance: string) -> object,
//     claims_authority(utterance: string) -> object,
//     validate_reply(reply: string, status: string) -> object,
//     normalized_hash(utterance: string) -> object,
//     createGovernor() -> {
//       decide(input: object) -> object,
//       revise_claim(input: object) -> object,
//       recent_receipts(n: number) -> object,
//       select_provider(input: object) -> object,
//       consume_provider_decision(decision: object) -> object,
//       record_provider_result(input: object) -> object,
//       record_state_transition(input: object) -> object,
//       verify_chain(receipts: object[]) -> object,
//       free() -> void,
//     }
//   }>
//
// Every call returns parsed JSON. A Rust panic surfaces as a thrown Error —
// the caller MUST drop the governor and create a new one on any throw
// (a poisoned mutex never recovers).

export async function loadGovernor(wasmBytes) {
  const imports = {
    env: {
      indy_host_now_ms: () => Date.now(),
    },
    __wbindgen_placeholder__: {
      __wbindgen_describe: () => {},
      __wbg___wbindgen_throw_41e9ee4f547fc59a: (ptr, len) => {
        throw new Error('governor wasm: ' + readStr(ptr, len));
      },
    },
    __wbindgen_externref_xform__: {
      __wbindgen_externref_table_set_null: () => {},
      __wbindgen_externref_table_grow: () => 0,
    },
  };

  let memoryU8, memoryU32;
  const readStr = (ptr, len) =>
    new TextDecoder().decode(memoryU8().subarray(ptr, ptr + len));

  // WebAssembly.instantiate has two overloads: bytes -> {module, instance},
  // precompiled Module -> Instance directly. Workers only allows the latter.
  const instantiated = await WebAssembly.instantiate(wasmBytes, imports);
  const instance =
    instantiated instanceof WebAssembly.Instance ? instantiated : instantiated.instance;
  const ex = instance.exports;
  const names = Object.keys(ex);
  memoryU8 = () => new Uint8Array(ex.memory.buffer);
  memoryU32 = () => new Uint32Array(ex.memory.buffer);

  const malloc = ex.__wbindgen_malloc;
  const free = ex.__wbindgen_free;
  if (typeof malloc !== 'function' || typeof free !== 'function' || !ex.memory) {
    throw new Error('governor wasm: missing malloc/free/memory exports');
  }

  const findFn = (prefix) => {
    for (const n of names) {
      if ((n === prefix || n.startsWith(prefix + '_')) && typeof ex[n] === 'function') return ex[n];
    }
    return null;
  };

  // f(retptr, ptr, len) -> JSON string (wasm-bindgen &str in / String out)
  const callStr = (fn, input) => {
    const data = new TextEncoder().encode(input);
    const ptr = malloc(data.length, 1);
    memoryU8().set(data, ptr);
    const retptr = malloc(8, 1);
    try {
      fn(retptr, ptr, data.length);
      const u32 = memoryU32();
      const outPtr = u32[retptr / 4], outLen = u32[retptr / 4 + 1];
      const out = readStr(outPtr, outLen);
      free(outPtr, outLen, 1);
      return out;
    } finally {
      free(retptr, 8, 1);
    }
  };

  const callJson = (fn, obj) => JSON.parse(callStr(fn, JSON.stringify(obj)));

  // f(retptr) -> JSON string (wasm-bindgen unit-arg convention)
  const callUnit = (fn) => {
    const retptr = malloc(8, 1);
    try {
      fn(retptr);
      const u32 = memoryU32();
      const outPtr = u32[retptr / 4], outLen = u32[retptr / 4 + 1];
      const out = readStr(outPtr, outLen);
      free(outPtr, outLen, 1);
      return out;
    } finally {
      free(retptr, 8, 1);
    }
  };

  // WasmGovernor: raw wasm-bindgen shims (no JS glue without the CLI).
  //   wasmgovernor_new() -> u32 self_ptr
  //   wasmgovernor_<method>(retptr, self_ptr, arg_ptr, arg_len)
  //   wasmgovernor_recent_receipts(retptr, self_ptr, n: u32)
  //   __wbg_wasmgovernor_free(self_ptr)
  const shimNew = findFn('wasmgovernor_new');
  const shimFree = findFn('__wbg_wasmgovernor_free');
  if (!shimNew || !shimFree) throw new Error('governor wasm: missing governor new/free shims');

  // f(retptr, self_ptr, ptr, len) -> JSON string
  const callSelfStr = (fn, selfPtr, input) => {
    const data = new TextEncoder().encode(input);
    const ptr = malloc(data.length, 1);
    memoryU8().set(data, ptr);
    const retptr = malloc(8, 1);
    try {
      fn(retptr, selfPtr, ptr, data.length);
      const u32 = memoryU32();
      const outPtr = u32[retptr / 4], outLen = u32[retptr / 4 + 1];
      const out = readStr(outPtr, outLen);
      free(outPtr, outLen, 1);
      return out;
    } finally {
      free(retptr, 8, 1);
    }
  };

  const wrapGovernor = (selfPtr) => {
    const m = (name) => {
      const fn = findFn('wasmgovernor_' + name);
      if (!fn) throw new Error(`governor wasm: missing method shim ${name}`);
      return fn;
    };
    const decide = m('decide'), revise_claim = m('revise_claim'),
      select_provider = m('select_provider'),
      consume_provider_decision = m('consume_provider_decision'),
      record_provider_result = m('record_provider_result'),
      record_state_transition = m('record_state_transition'),
      verify_chain = m('verify_chain');
    const recent_receipts = m('recent_receipts');
    let freed = false;
    return {
      decide: (input) => JSON.parse(callSelfStr(decide, selfPtr, JSON.stringify(input))),
      revise_claim: (input) => JSON.parse(callSelfStr(revise_claim, selfPtr, JSON.stringify(input))),
      recent_receipts: (n) => {
        const retptr = malloc(8, 1);
        try {
          recent_receipts(retptr, selfPtr, n);
          const u32 = memoryU32();
          const outPtr = u32[retptr / 4], outLen = u32[retptr / 4 + 1];
          const out = readStr(outPtr, outLen);
          free(outPtr, outLen, 1);
          return JSON.parse(out);
        } finally {
          free(retptr, 8, 1);
        }
      },
      select_provider: (input) => JSON.parse(callSelfStr(select_provider, selfPtr, JSON.stringify(input))),
      consume_provider_decision: (decision) => JSON.parse(callSelfStr(consume_provider_decision, selfPtr, JSON.stringify(decision))),
      record_provider_result: (input) => JSON.parse(callSelfStr(record_provider_result, selfPtr, JSON.stringify(input))),
      record_state_transition: (input) => JSON.parse(callSelfStr(record_state_transition, selfPtr, JSON.stringify(input))),
      verify_chain: (receipts) => JSON.parse(callSelfStr(verify_chain, selfPtr, JSON.stringify(receipts))),
      free: () => { if (!freed) { freed = true; shimFree(selfPtr); } },
    };
  };

  const fns = {
    classify_intent: findFn('classify_intent'),
    claims_authority: findFn('claims_authority'),
    validate_reply: findFn('validate_reply'),
    normalized_hash: findFn('normalized_hash'),
    mcp_descriptors: findFn('mcp_descriptors'),
  };
  for (const [k, v] of Object.entries(fns)) {
    if (!v) throw new Error(`governor wasm: missing export ${k}`);
  }

  // (wrapGovernor defined above)
  // WasmMcp: raw shims.
  //   wasmmcp_new() -> u32 self_ptr
  //   wasmmcp_call_tool(retptr, self_ptr, name_ptr, name_len, args_ptr, args_len)
  //   __wbg_wasmmcp_free(self_ptr)
  const mcpShimNew = findFn('wasmmcp_new');
  const mcpShimFree = findFn('__wbg_wasmmcp_free');
  const mcpShimCall = findFn('wasmmcp_call_tool');
  if (!mcpShimNew || !mcpShimFree || !mcpShimCall) {
    throw new Error('governor wasm: missing mcp shims');
  }

  const callMcpTool = (selfPtr, name, argsJson) => {
    const nData = new TextEncoder().encode(name);
    const aData = new TextEncoder().encode(argsJson);
    const nPtr = malloc(nData.length, 1);
    const aPtr = malloc(aData.length, 1);
    memoryU8().set(nData, nPtr);
    memoryU8().set(aData, aPtr);
    const retptr = malloc(8, 1);
    try {
      mcpShimCall(retptr, selfPtr, nPtr, nData.length, aPtr, aData.length);
      const u32 = memoryU32();
      const outPtr = u32[retptr / 4], outLen = u32[retptr / 4 + 1];
      const out = readStr(outPtr, outLen);
      free(outPtr, outLen, 1);
      return out;
    } finally {
      free(retptr, 8, 1);
    }
  };

  const wrapMcp = (selfPtr) => {
    let freed = false;
    return {
      call_tool: (name, args) => JSON.parse(callMcpTool(selfPtr, name, JSON.stringify(args))),
      free: () => { if (!freed) { freed = true; mcpShimFree(selfPtr); } },
    };
  };

  return {
    classify_intent: (utterance) => callJson(fns.classify_intent, { utterance }),
    claims_authority: (utterance) => callJson(fns.claims_authority, { utterance }),
    validate_reply: (reply, status) => callJson(fns.validate_reply, { reply, status }),
    normalized_hash: (utterance) => callJson(fns.normalized_hash, { utterance }),
    mcp_descriptors: () => JSON.parse(callUnit(fns.mcp_descriptors)),
    createGovernor: () => wrapGovernor(shimNew()),
    createMcp: () => wrapMcp(mcpShimNew()),
  };
}

// loadNodeGovernor(): node convenience — reads the wasm next to this file.
// Tries governor_wasm.wasm beside loader.mjs, then the cargo target path.
export async function loadNodeGovernor() {
  const { readFile } = await import('node:fs/promises');
  const { fileURLToPath } = await import('node:url');
  const { dirname, join } = await import('node:path');
  const dir = dirname(fileURLToPath(import.meta.url));
  const candidates = [
    join(dir, 'governor_wasm.wasm'),
    join(dir, 'target/wasm32-unknown-unknown/release/governor_wasm.wasm'),
  ];
  let bytes = null, lastErr = null;
  for (const p of candidates) {
    try { bytes = await readFile(p); break; } catch (e) { lastErr = e; }
  }
  if (!bytes) throw new Error('governor wasm not found: ' + lastErr);
  return loadGovernor(bytes);
}
