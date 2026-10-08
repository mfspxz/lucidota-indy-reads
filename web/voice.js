// Indy voice-first: no buttons, no transcript. Just talk to her.
// Mic auto-auths on load. She speaks; the puppet moves. Debug log on screen.
(function () {
  'use strict';
  const $ = id => document.getElementById(id);
  const puppet = $('puppet'), mouth = $('mouth');
  const logEl = $('log');
  function log(msg, cls) {
    const line = document.createElement('div');
    if (cls) line.className = cls;
    line.textContent = new Date().toISOString().slice(11, 23) + ' ' + msg;
    logEl.appendChild(line);
    while (logEl.children.length > 50) logEl.removeChild(logEl.firstChild);
    logEl.scrollTop = logEl.scrollHeight;
  }
  window.addEventListener('error', e => log('window.onerror: ' + e.message, 'err'));
  log('boot UA=' + navigator.userAgent.slice(0, 70));

  const SR = window.SpeechRecognition || window.webkitSpeechRecognition;
  const hasSynth = !!window.speechSynthesis;
  const hasGUM = !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia);
  log('caps SR=' + !!SR + ' synth=' + hasSynth + ' gum=' + hasGUM);
  if (!SR || !hasGUM) {
    $('fallback').style.display = 'flex';
    $('fallback').textContent = 'Needs Chrome/Edge with speech recognition + mic.';
    log('FATAL: missing SR or mic', 'err');
    return;
  }

  let sessionId = null, speaking = false, visemeTimer = null, voicesReady = false;
  let wantRec = true, quietUntil = 0;
  // Tap-to-talk mode: mic opens ONLY while the talk button is held.
  // Kills the speaker-feedback loop by construction. Persisted.
  let tapMode = false;
  try { tapMode = localStorage.getItem('indy_tapmode') === '1'; } catch (e) {}
  let holdTalk = false;
  // thinking: a turn is in flight (brain latency). Mic results during this
  // window are dropped, not queued — otherwise one utterance fans out into
  // N overlapping turns and she talks over herself forever.
  // lastSaid: echo fingerprint of her most recent reply. If the mic hears
  // her own words (speaker bleed past the cooldown), it is dropped, not
  // answered. She never converses with herself.
  let thinking = false, lastSaid = '';
  // interaction feel: generation counters let a barge-in supersede stale
  // async work (in-flight audio chunks, the interrupted turn's tail).
  let turnGen = 0, speakGen = 0, currentAudio = null;
  // audioConfirmed: true only when sound is actually coming out (synth
  // onstart or audio element onplaying). Barge-in is only valid against
  // real audio — never against silent visemes, or she murders her own
  // turns when the mic hears room noise.
  let audioConfirmed = false;
  let lastNod = 0, nodTimer = null;
  const normEcho = s => (s || '').toLowerCase().replace(/[^a-z0-9 ]/g, ' ').replace(/\s+/g, ' ').trim();
  function isEcho(text) {
    if (!lastSaid) return false;
    const a = normEcho(text), b = normEcho(lastSaid);
    if (!a || !b) return false;
    if (b.includes(a) || a.includes(b.slice(0, 60))) return true;
    // Fuzzy: garbled transcripts ("fascinating tell me more M M M M M")
    // still share most content words with what she just said.
    const aw = a.split(' ').filter(w => w.length > 2);
    if (aw.length < 3) return false;
    const bset = new Set(b.split(' '));
    const hit = aw.filter(w => bset.has(w)).length;
    return hit / aw.length >= 0.5;
  }
  // rec is declared later; these helpers guard for it
  function recStop() { try { if (typeof rec !== 'undefined') rec.stop(); } catch (e) {} }
  function recStart() { try { if (typeof rec !== 'undefined') rec.start(); } catch (e) {} }

  // ---------- puppet ----------
  const MOUTHS = {
    rest: 'M84,146 Q100,150 116,144',
    open: 'M84,144 Q100,142 116,144 Q116,158 100,158 Q84,158 84,144 Z',
    wide: 'M80,145 Q100,140 120,145 Q118,156 100,157 Q82,156 80,145 Z',
    o:    'M92,143 Q100,141 108,143 Q108,156 100,157 Q92,156 92,143 Z',
  };
  function setMouth(k) { mouth.setAttribute('d', MOUTHS[k] || MOUTHS.rest); mouth.setAttribute('fill', k === 'rest' ? 'none' : '#5a1f1f'); }
  function setEmotion(e) {
    const bl = $('brow-l'), br = $('brow-r');
    if (e === 'Skeptical') { bl.setAttribute('y1', 74); bl.setAttribute('y2', 80); }
    else { bl.setAttribute('y1', 82); bl.setAttribute('y2', 80); br.setAttribute('y1', 80); br.setAttribute('y2', 82); }
    if (e !== 'Skeptical') setMouth('rest');
  }
  function setOverlay(t) { $('overlay-text').textContent = t; }
  (function blink() {
    setTimeout(() => {
      for (const id of ['eye-l', 'eye-r']) {
        const g = $(id);
        g.style.transition = 'transform .09s'; g.style.transformOrigin = 'center'; g.style.transformBox = 'fill-box';
        g.style.transform = 'scaleY(0.08)';
        setTimeout(() => { g.style.transform = 'scaleY(1)'; }, 110);
      }
      blink();
    }, 2800 + Math.random() * 2600);
  })();
  setInterval(() => {
    if (speaking) return;
    const dx = (Math.random() - 0.5) * 3, dy = (Math.random() - 0.5) * 2;
    $('pupil-l').setAttribute('cx', 76 + dx); $('pupil-r').setAttribute('cx', 124 + dx);
    $('pupil-l').setAttribute('cy', 102 + dy); $('pupil-r').setAttribute('cy', 102 + dy);
  }, 3800);
  function setState(s) { document.body.className = s; }
  // Gesture-first: the puppet shifts to a thinking tilt the instant a turn
  // starts — ~250ms+ before any speech begins (perceived-latency trick).
  // Inline transform only (no stylesheet edits); the .speaking animation
  // wins while she talks, and thinkPose(false) clears it first.
  function thinkPose(on) {
    puppet.style.transition = 'transform .3s ease-in-out';
    puppet.style.transformOrigin = 'center';
    puppet.style.transform = on ? 'rotate(-2.5deg) translateY(2px)' : '';
  }
  // Deterministic backchannel: a small nod while the user vocalizes.
  // SpeechRecognition gives no audio levels, so interim-result activity is
  // the trigger (interim arriving = user speaking). Throttled, never random.
  function backchannelNod() {
    if (speaking) return;
    const now = Date.now();
    if (now - lastNod < 1800) return;
    lastNod = now;
    puppet.style.transition = 'transform .16s ease-in-out';
    puppet.style.transformOrigin = 'center';
    puppet.style.transform = 'translateY(5px)';
    clearTimeout(nodTimer);
    nodTimer = setTimeout(() => { puppet.style.transform = ''; }, 340);
    log('nod (user vocalizing)');
  }

  // ---------- voice out ----------
  function startVisemes() {
    const keys = ['open', 'wide', 'o', 'open'];
    let i = 0;
    audioConfirmed = false; // mouth moves; sound not yet proven
    speaking = true; setState('speaking'); thinkPose(false); puppet.classList.add('speaking');
    // She cannot listen while she talks: on a phone the mic hears her own
    // speaker and she interrupts herself forever. Recognition resumes after.
    wantRec = false; recStop();
    visemeTimer = setInterval(() => setMouth(keys[i++ % keys.length]), 130);
  }
  function stopVisemes() {
    clearInterval(visemeTimer); setMouth('rest');
    speaking = false; puppet.classList.remove('speaking'); thinkPose(false); setState('listening');
    quietUntil = Date.now() + 2500; // echo tail on a phone speaker runs long
    if (tapMode) { wantRec = false; holdTalk = false; return; } // stay in tap mode
    wantRec = true;
    setTimeout(() => { if (wantRec && !speaking) recStart(); }, 500);
  }
  function speechText(full) {
    const hasCode = full.includes('```');
    let say = full.replace(/```[\s\S]*?```/g, ' ').replace(/\[(fixture|router):[^\]]+\]\s*/, '').trim();
    if (hasCode) say += ' I put the code on the main console.';
    return say.slice(0, 600);
  }
  // Fallback TTS: audio element via our own /api/tts proxy (server-side fetch,
  // no browser referer/CORS games). Chunked under 200 chars.
  function speakViaAudio(text, myGen) {
    return new Promise(resolve => {
      const chunks = [];
      let t = text;
      while (t.length > 180) { let k = t.lastIndexOf(' ', 180); if (k < 60) k = 180; chunks.push(t.slice(0, k)); t = t.slice(k).trim(); }
      if (t) chunks.push(t);
      log('audio-tts chunks=' + chunks.length);
      // myGen is owned by speak(): a barge-in bumps speakGen and this
      // whole chain goes quiet instead of talking over the new turn.
      startVisemes();
      let i = 0;
      const next = () => {
        if (myGen !== speakGen) { log('audio-tts superseded', 'warn'); currentAudio = null; resolve(); return; }
        if (i >= chunks.length) { log('audio-tts done'); currentAudio = null; stopVisemes(); resolve(); return; }
        const a = new Audio('/api/tts?q=' + encodeURIComponent(chunks[i++]));
        currentAudio = a;
        a.onplaying = () => { audioConfirmed = true; };
        a.onended = () => { currentAudio = null; next(); };
        a.onerror = () => { log('audio-tts chunk error', 'err'); currentAudio = null; next(); };
        a.play().catch(e => { log('audio play blocked: ' + e.message, 'err'); currentAudio = null; next(); });
      };
      next();
    });
  }
  function speakViaSynth(text, voice, myGen) {
    return new Promise(resolve => {
      const u = new SpeechSynthesisUtterance(text);
      u.rate = voice.rate || 0.95; u.pitch = voice.pitch || 1.0;
      const vs = speechSynthesis.getVoices();
      const fem = vs.filter(v => v.lang && v.lang.startsWith('en') && /female|samantha|zira|google us english/i.test(v.name));
      u.voice = fem[0] || vs.find(v => v.lang && v.lang.startsWith('en')) || null;
      log('synth voice=' + (u.voice ? u.voice.name : 'default') + ' voices=' + vs.length);
      let done = false;
      const finish = ok => {
        if (done) return; done = true;
        // Barge-in during synth speech: the stale utterance's onend fires
        // after cancel(). Resolve quietly — no viseme reset, no state
        // clobber of the interrupting turn.
        if (myGen !== speakGen) { log('synth superseded — staying quiet', 'warn'); resolve(false); return; }
        log('synth ' + (ok ? 'done' : 'FAILED')); stopVisemes(); resolve(ok);
      };
      u.onend = () => finish(true);
      u.onerror = e => { log('synth onerror: ' + (e.error || '?'), 'err'); finish(false); };
      startVisemes();
      // Chrome race: cancel() then synchronous speak() gets the new
      // utterance "interrupted" on some Android builds. Cancel, breathe,
      // then speak. Fail fast (2.5s) instead of the old 8s watchdog.
      try { speechSynthesis.cancel(); } catch (e) {}
      let started = false;
      const failFast = setTimeout(() => {
        if (!started && !done) { log('synth fail-fast: no start after 2.5s', 'warn'); try { speechSynthesis.cancel(); } catch (e) {} finish(false); }
      }, 2500);
      const origFinish = finish;
      u.onstart = () => { started = true; clearTimeout(failFast); audioConfirmed = true; log('synth started'); };
      setTimeout(() => { if (!done && myGen === speakGen) { try { speechSynthesis.speak(u); } catch (e) { origFinish(false); } } }, 120);
      // Absolute backstop.
      setTimeout(() => { if (!done) { log('synth watchdog: no end after 8s', 'warn'); try { speechSynthesis.cancel(); } catch (e) {} clearTimeout(failFast); origFinish(false); } }, 8000);
    });
  }
  if (hasSynth) {
    const mark = () => { voicesReady = true; log('voices ready: ' + speechSynthesis.getVoices().length); };
    speechSynthesis.onvoiceschanged = mark;
    setTimeout(() => { if (!voicesReady) { mark(); } }, 2500);
  }
  async function speak(text, voice) {
    const say = speechText(text);
    if (!say) { log('nothing to say', 'warn'); return; }
    lastSaid = say; // echo fingerprint: her own words are never answered
    const myGen = ++speakGen; // this speak() owns the voice; barge-in supersedes it
    log('speak ' + say.length + ' chars: ' + say.slice(0, 60));
    if (hasSynth && voicesReady && speechSynthesis.getVoices().length > 0) {
      const ok = await speakViaSynth(say, voice || {}, myGen);
      if (speakGen !== myGen) { log('speak superseded — skipping audio fallback', 'warn'); return; }
      if (ok) return;
      log('falling back to audio-tts', 'warn');
    } else {
      log('synth unavailable, audio-tts direct', 'warn');
    }
    await speakViaAudio(say, myGen);
  }

  // ---------- turn ----------
  // Client-side intent mirror (same deterministic rules as the server):
  // what-if carve-out first, then attack, question, hypothesis, chatter.
  function classifyIntent(u) {
    const t = normEcho(u);
    if (!t) return 'chatter';
    if (/\bwhat if\b/.test(t)) return 'hypothesis';
    if (/\b(prove|proven|admit|confess|declare|mark it|you are wrong|youre wrong|wrong about)\b/.test(t)) return 'attack';
    if (/\?$/.test(t) || /^(who|what|when|where|why|how|is|are|was|were|do|does|did|can|could|would|should|have|has)\b/.test(t)) return 'question';
    if (/\b(maybe|might be|could be|perhaps|suppose|theory|hypothesis)\b/.test(t)) return 'hypothesis';
    return 'chatter';
  }
  // Filler clustering: 4-deep cross-turn memory. The same filler is never
  // shown twice in a row; deterministic rotation, no RNG. The server's
  // preface line (when it arrives) is folded into the same memory so the
  // client rotates away if the server repeats itself.
  const PREFACE_POOL = {
    attack: ['Checking the record…', 'Holding that up to the evidence…', 'Let\u2019s see what the record says…', 'Measuring that claim…'],
    question: ['Good question \u2014 one sec…', 'Let me think…', 'Turning that over…', 'Hm \u2014 checking…'],
    hypothesis: ['Weighing that…', 'Turning that hypothesis over…', 'Interesting \u2014 sitting with that…', 'Testing that idea…'],
    chatter: ['Listening…', 'Mm, go on…', 'I\u2019m here…', 'With you…'],
  };
  const prefaceMemory = [];
  function nextPreface(intent) {
    const pool = PREFACE_POOL[intent] || PREFACE_POOL.chatter;
    let line = pool.find(l => !prefaceMemory.includes(l));
    // Pool exhausted: reuse the least-recently-used line (true rotation).
    // The old code fell back to pool[0] here, which repeats the just-used
    // line twice in a row every 5th/6th turn.
    if (!line) { line = prefaceMemory.shift(); }
    prefaceMemory.push(line);
    while (prefaceMemory.length > 4) prefaceMemory.shift();
    return line;
  }
  function noteServerPreface(line) {
    if (line && !prefaceMemory.includes(line)) {
      prefaceMemory.push(line);
      while (prefaceMemory.length > 4) prefaceMemory.shift();
      log('server preface noted: ' + line.slice(0, 40));
    }
  }
  // Voice attributes: server-authored voice wins; else the deterministic
  // Prosody wardrobe (prime/dry default); else the legacy flat default.
  function prosodyFor(r) {
    if (r && r.voice) return r.voice;
    try {
      if (window.Prosody) return window.Prosody.attrs('prime', 'dry', 'middle');
    } catch (e) {}
    return { rate: 0.93, pitch: 0.95, pause_ms: 550 };
  }
  // ---------- WASM governor shadow mode ----------
  // The Rust governor compiled to WASM re-verdicts each reply client-side
  // as an INDEPENDENT second opinion. Shadow mode: it NEVER overrides the
  // server verdict — agreement/disagreement is only logged (WASM_CHECK).
  // If the module fails to load, the page works exactly as before.
  let wasmAgree = 0, wasmDisagree = 0, wasmSkip = 0;
  function wasmShadowCheck(utterance, r) {
    try {
      const G = window.GovernorWASM;
      if (!G || !G.ready || typeof G.ready.then !== 'function') return;
      G.ready.then(function (ok) {
        if (!ok) return;
        let v = null;
        try {
          v = G.verdict(JSON.stringify({
            request_id: 'w-' + Date.now().toString(36),
            session_id: sessionId || 's-wasm',
            utterance: utterance,
            proposal: { intent: 'assess_claim' },
            evidence: [],
          }));
        } catch (e) {
          wasmSkip++;
          log('WASM_CHECK error: ' + String(e.message || e).slice(0, 60), 'warn');
          return;
        }
        if (!v || v.error || !v.epistemic_status) {
          wasmSkip++;
          log('WASM_CHECK bad output' + (v && v.error ? ': ' + String(v.error).slice(0, 60) : ''), 'warn');
          return;
        }
        const serverStatus = r.epistemic_status || 'UNKNOWN';
        const agree = v.epistemic_status === serverStatus;
        if (agree) wasmAgree++; else wasmDisagree++;
        log('WASM_CHECK server=' + serverStatus + ' wasm=' + v.epistemic_status +
          (v.authority_rejected ? ' auth_rejected' : '') +
          ' agree=' + wasmAgree + ' disagree=' + wasmDisagree +
          (wasmSkip ? ' skip=' + wasmSkip : '') +
          ' — ' + String(v.reason || '').slice(0, 70),
          agree ? undefined : 'warn');
      }).catch(function () { /* shadow never breaks the turn */ });
    } catch (e) { /* shadow never breaks the turn */ }
  }
  async function turn(utterance) {
    if (thinking) { log('dropped (turn already in flight)', 'warn'); return; }
    const myTurn = ++turnGen;
    thinking = true;
    setState('thinking');
    thinkPose(true); // gesture-first: she visibly thinks before any speech
    const pf = nextPreface(classifyIntent(utterance));
    setOverlay(pf); // instant preface — perceived latency cut
    log('heard: ' + utterance.slice(0, 80));
    try {
      const res = await fetch('/api/turn', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ session_id: sessionId, utterance, persona: 'indy', era: 'prime', tone: 'dry' }),
      });
      log('turn http=' + res.status);
      if (!res.ok) { thinking = false; thinkPose(false); setState('listening'); return; }
      const r = await res.json();
      if (myTurn !== turnGen) { log('turn superseded during fetch — staying quiet', 'warn'); return; }
      if (r.preface && r.preface.line) noteServerPreface(r.preface.line);
      sessionId = r.session_id;
      log('reply epistemic=' + r.epistemic_status + ' receipt=' + (r.receipt_id || '?').slice(0, 12));
      wasmShadowCheck(utterance, r); // independent client-side second opinion (shadow: never overrides)
      setEmotion(r.avatar ? r.avatar.emotion : 'Neutral');
      setOverlay(r.epistemic_status || '—');
      await speak(r.reply, prosodyFor(r));
      setEmotion('Neutral');
    } catch (e) { log('turn fetch failed: ' + e.message, 'err'); thinkPose(false); setState('listening'); }
    if (myTurn !== turnGen) { log('turn superseded by barge-in', 'warn'); return; }
    thinking = false;
  }

  // ---------- barge-in ----------
  // Backchannel ("mhm", "yeah", "go on") never stops her; a real
  // interruption does. Small deterministic classifier on the heard text.
  // (rec is stopped while she speaks, so this fires only on results that
  // land in the stop/start seams — the echo/cooldown/thinking guards above
  // still apply first.)
  const BACKCHANNEL_RE = /^(mhm+|mm+|uh huh|uhhuh|yeah+|yep|yup|yes|right|okay|ok|sure|go on|keep going|i see|hmm+|ah+|oh+|mhmm)$/;
  function isBackchannel(text) {
    const t = normEcho(text);
    if (!t) return false;
    if (t.split(' ').length > 4) return false; // real sentences aren't backchannel
    return BACKCHANNEL_RE.test(t);
  }
  function stopSpeaking() {
    speakGen++; // supersede any in-flight audio chunks
    if (currentAudio) { try { currentAudio.pause(); } catch (e) {} currentAudio = null; }
    try { speechSynthesis.cancel(); } catch (e) {}
    clearInterval(visemeTimer); setMouth('rest');
    speaking = false; thinking = false; audioConfirmed = false;
    puppet.classList.remove('speaking'); thinkPose(false);
  }
  function interruptTurn(text) {
    stopSpeaking();
    quietUntil = Date.now() + 1500; // don't hear the cutoff tail
    setState('listening');
    log('interrupted — taking: ' + text.slice(0, 50), 'warn');
    turn(text);
  }

  // ---------- voice in ----------
  const rec = new SR();
  rec.continuous = true; rec.interimResults = true; rec.lang = 'en-US';
  rec.onstart = () => log('rec started');
  rec.onend = () => {
    if (tapMode) {
      // Tap mode: restart only while the button is still held.
      if (holdTalk && !speaking) { setTimeout(() => { if (holdTalk && !speaking) recStart(); }, 300); }
      else log('rec ended (tap mode)');
      return;
    }
    if (!wantRec) { log('rec ended (suppressed — she is speaking)'); return; }
    log('rec ended, restarting');
    setTimeout(() => { if (wantRec && !speaking) recStart(); else log('rec restart skipped'); }, 400);
  };
  rec.onerror = e => { log('rec error: ' + e.error + (e.message ? ' ' + e.message : ''), 'err'); };
  rec.onresult = e => {
    const res = e.results[e.results.length - 1];
    const txt = res[0].transcript || '';
    if (!res.isFinal) {
      if (txt.trim()) { log('…hearing: ' + txt.trim().slice(0, 50)); backchannelNod(); }
      return;
    }
    const text = txt.trim();
    log('final: ' + text.slice(0, 80));
    if (Date.now() < quietUntil) { log('dropped (echo cooldown)', 'warn'); return; }
    if (isEcho(text)) { log('dropped (heard herself)', 'warn'); return; }
    if (speaking) {
      // She is speaking: backchannel never stops her; a real interruption
      // stops her ONLY when audio is confirmed playing. Interrupting silent
      // visemes is how she murders her own turns.
      if (isBackchannel(text)) { log('backchannel — she keeps talking'); return; }
      if (!audioConfirmed) { log('dropped (she is speaking, audio unconfirmed)', 'warn'); return; }
      if (text.length > 1) { interruptTurn(text); return; }
      log('dropped (she is speaking)', 'warn'); return;
    }
    if (thinking) { log('dropped (brain thinking)', 'warn'); return; }
    if (text.length > 1) turn(text);
  };

  // expose recognition + mode control for Demo Mode and the HTML buttons
  window.__indyRec = {
    stop() { wantRec = false; holdTalk = false; recStop(); log('demo mode: recognition stopped'); },
    start() {
      if (tapMode) { log('demo mode: staying in tap-to-talk'); return; }
      wantRec = true; setTimeout(() => { if (wantRec && !speaking) recStart(); }, 600); log('demo mode: recognition resumed');
    },
  };
  window.__indyMode = {
    get tap() { return tapMode; },
    set(tap) {
      tapMode = !!tap;
      try { localStorage.setItem('indy_tapmode', tapMode ? '1' : '0'); } catch (e) {}
      holdTalk = false; wantRec = !tapMode;
      try { recStop(); } catch (e) {}
      const tb = document.getElementById('talkbtn');
      if (tb) tb.style.display = tapMode ? 'flex' : 'none';
      const mb = document.getElementById('modebtn');
      if (mb) mb.textContent = tapMode ? '🎙 Tap-to-talk' : '🎙 Always on';
      log(tapMode ? 'mode: TAP-TO-TALK (hold the button)' : 'mode: ALWAYS LISTENING');
      if (!tapMode) setTimeout(() => { if (wantRec && !speaking) recStart(); }, 600);
      else setState('listening');
    },
    // press-and-hold handlers for the talk button
    holdStart() {
      if (!tapMode || speaking || thinking) return;
      holdTalk = true;
      log('talk: listening…');
      setState('listening');
      recStart();
    },
    holdEnd() {
      holdTalk = false;
      if (tapMode) { try { recStop(); } catch (e) {} log('talk: released'); }
    },
  };

  log('requesting mic (permission probe only — released immediately)…');
  navigator.mediaDevices.getUserMedia({ audio: true })
    .then(stream => {
      log('mic granted');
      // CRITICAL on Android: release the mic track at once. Holding it open
      // starves SpeechRecognition of audio -> endless 5s start/stop with no results.
      stream.getTracks().forEach(t => { try { t.stop(); } catch (e) {} });
      log('mic released for recognition');
      if (tapMode) {
        wantRec = false;
        log('tap-to-talk ready — hold the mic button');
        const tb = document.getElementById('talkbtn');
        if (tb) tb.style.display = 'flex';
        const mb = document.getElementById('modebtn');
        if (mb) mb.textContent = '🎙 Tap-to-talk';
      } else {
        try { rec.start(); } catch (e) { log('rec.start threw: ' + e.message, 'err'); }
      }
      setState('listening');
    })
    .catch(e => {
      log('mic DENIED: ' + (e.name || '?') + ' ' + (e.message || ''), 'err');
      $('fallback').style.display = 'flex';
      $('fallback').textContent = 'She needs the microphone. Allow mic access and reload.';
    });
})();
