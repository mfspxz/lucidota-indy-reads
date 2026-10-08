// Indy_Reads frontend: voice in, governor visualization, receipts, voice out.
// API base override: open the page as /?api=https://your-server to point the
// UI at a remote governor (e.g. Vercel-hosted UI + laptop server).
let sessionId = null;
let turns = 0;
const chat = document.getElementById('chat');
const stages = document.querySelectorAll('.stage');

function stage(name, cls) {
  stages.forEach(s => {
    if (s.dataset.s === name) { s.classList.remove('active','done'); if (cls) s.classList.add(cls); }
  });
  // Sliding highlight bar + avatar stage icon: only ever driven by real turns.
  const bar = document.getElementById('stepbar');
  const icon = document.getElementById('stageicon');
  const ICONS = { input:'🎙', stt:'💬', govern:'⚖️', epistemic:'❓', ai:'✨', validate:'✅', response:'🗣', receipt:'🧾' };
  if (cls === 'active' && bar) {
    const el = document.querySelector('.stage[data-s="' + name + '"]');
    if (el) {
      const pipe = document.getElementById('pipeline');
      const pr = pipe.getBoundingClientRect(), r = el.getBoundingClientRect();
      bar.style.top = (r.top - pr.top) + 'px';
      bar.style.height = r.height + 'px';
      bar.classList.add('on');
    }
    if (icon && ICONS[name]) {
      icon.textContent = ICONS[name];
      icon.title = 'driving subsystem: ' + name.toUpperCase();
      icon.classList.add('on');
    }
  }
}
function clearStageFx() {
  const bar = document.getElementById('stepbar');
  const icon = document.getElementById('stageicon');
  if (bar) bar.classList.remove('on');
  if (icon) icon.classList.remove('on');
}
function allStages(cls){ stages.forEach(s=>{s.classList.remove('active','done'); if(cls) s.classList.add(cls);}); clearStageFx(); }
function setStageTip(name, tip) {
  stages.forEach(s => { if (s.dataset.s === name && tip) s.title = tip; });
}

function addMsg(who, text, prov) {
  const d = document.createElement('div');
  d.className = 'msg ' + (who === 'YOU' ? 'user' : 'indy');
  d.innerHTML = `<div class="who">${who}</div><div></div>`;
  // Render ```code fences``` as <pre> blocks (textContent throughout — no HTML sink).
  const body = d.lastChild;
  const parts = text.split('```');
  parts.forEach((p, i) => {
    if (i % 2 === 1) {
      const pre = document.createElement('pre');
      pre.className = 'codeblock';
      pre.textContent = p.replace(/^\w+\n/, '');
      body.appendChild(pre);
    } else if (p) {
      const span = document.createElement('span');
      span.textContent = p;
      body.appendChild(span);
    }
  });
  if (prov && who !== 'YOU') {
    const f = document.createElement('div');
    f.className = 'prov';
    f.innerHTML = '';
    const b = document.createElement('b'); b.textContent = 'SYNTHETIC';
    f.appendChild(b);
    f.appendChild(document.createTextNode(' · ' + prov));
    d.appendChild(f);
  }
  chat.appendChild(d);
  chat.scrollTop = chat.scrollHeight;
  return d;
}

const EPISTEMIC_STATES = ['UNKNOWN', 'PROVEN', 'DISPROVEN', 'HYPOTHESIS'];
function setEpistemic(status, detail) {
  // Hostile backends (?api=) must not be able to inject markup through the
  // epistemic status: whitelist the category, build DOM with textContent only.
  const s = EPISTEMIC_STATES.includes(String(status).toUpperCase())
    ? String(status).toUpperCase() : 'UNKNOWN';
  const el = document.getElementById('epistemic');
  el.innerHTML = '';
  const badge = document.createElement('div');
  badge.id = 'epibadge';
  badge.className = 's-' + s.toLowerCase();
  badge.title = 'epistemic state';
  const lab = document.createElement('div');
  lab.className = s.toLowerCase();
  lab.textContent = s;
  const det = document.createElement('div');
  det.className = 'detail';
  det.textContent = detail || '';
  el.append(badge, lab, det);
  // Pulse the badge once: the state actually changed. No fake confidence meter.
  badge.classList.add('pulse');
  setTimeout(() => badge.classList.remove('pulse'), 950);
}

// ?api= override: only http(s) backends. javascript:, data:, and other
// schemes are rejected — the override points at a governor, not at markup.
const _apiRaw = (new URLSearchParams(location.search).get('api') || '').replace(/\/$/, '');
const API_BASE = (/^https?:\/\//i.test(_apiRaw) || _apiRaw === '') ? _apiRaw : '';
function api(path) { return API_BASE + path; }

async function sendTurn(text) {
  if (!text.trim()) return;
  addMsg('YOU', text);
  allStages(); stage('input','active'); stage('stt','done');
  const t0 = performance.now();
  try {
    stage('govern','active');
    const res = await fetch(api('/api/turn'), {
      method: 'POST', headers: {'Content-Type':'application/json'},
      body: JSON.stringify({session_id: sessionId, utterance: text, persona: currentPersona(),
        era: document.getElementById('era')?.value || 'prime',
        tone: document.getElementById('tone')?.value || 'dry'}),
    });
    if (!res.ok) { addMsg('INDY', '[system] request failed: ' + res.status); allStages(); return; }
    const r = await res.json();
    sessionId = r.session_id; turns++;
    stage('govern','done'); stage('epistemic','done'); stage('ai','done'); stage('validate','done'); stage('response','active');
    // Provenance footer: every utterance declares itself synthetic, with its
    // voice id, era/tone costume, and the graph revision that authorized it.
    const eraTone = (document.getElementById('era')?.value || 'prime') + '/' +
                    (document.getElementById('tone')?.value || 'dry');
    const provLine = (r.voice ? r.voice.voice_id + ' · ' + eraTone : eraTone) +
      (r.provenance ? ' · rev ' + String(r.provenance.graph_revision).slice(0, 8) : '');
    addMsg('INDY', r.reply + (r.fixture ? '  [demo fixture — no provider key]' : ''), provLine);
    // Pipeline tooltips: the deterministic trace, from real response data.
    setStageTip('govern', 'mode ' + r.response_mode + ' · ' + (r.governor_reason || ''));
    setStageTip('epistemic', r.epistemic_status + ((r.rejected_fields||[]).length ? ' · forged fields rejected: ' + r.rejected_fields.join(', ') : ''));
    setStageTip('ai', 'persona ' + (r.persona || 'indy') + ' · ' + eraTone + (r.fixture ? ' · fixture (no key)' : ''));
    setStageTip('response', r.latency_ms + 'ms');
    if (r.provenance) setStageTip('receipt', 'receipt ' + String(r.provenance.graph_revision).slice(0, 8));
    if (r.avatar) {
      setEmotion(r.avatar.emotion);
      setOverlay(r.avatar.epistemic_overlay);
      document.getElementById('govreason').textContent += ' · avatar: ' + r.avatar.emotion.toLowerCase() + '/' + r.avatar.gesture.toLowerCase();
    }
    setEpistemic(r.epistemic_status, r.governor_reason);
    // Live truth reconciliation: a correction invalidated pending output.
    // Stop any in-flight speech immediately (fast lane), then flash the
    // retraction so the user sees the old -> new state change.
    if (r.speech_interrupted) {
      try { speechSynthesis.cancel(); } catch (e) {}
      if (typeof stopVisemes === 'function') stopVisemes();
      flashCorrection(r.revision);
    }
    // Voice state + provenance: the same entity across voice, face, graph.
    lastVoice = r.voice || null;
    if (r.voice) {
      document.getElementById('m-persona').textContent =
        'persona ' + (r.persona || 'indy') + ' · ' + r.voice.voice_id + ' · ' +
        (document.getElementById('era')?.value || 'prime') + '/' + (document.getElementById('tone')?.value || 'dry');
    }
    if (r.provenance) {
      document.getElementById('govreason').textContent +=
        ' · voice ' + r.provenance.provenance + '/' + r.provenance.voice_id +
        ' · rev ' + String(r.provenance.graph_revision).slice(0, 8);
    }
    // Continuity: the same entity across time.
    if (r.continuity && r.continuity.seen_before) {
      addMsg('INDY', '[continuity] assessed ' + r.continuity.previous_state +
        ' before · evidence items now ' + r.continuity.evidence_items_now);
    }
    document.getElementById('govreason').textContent = 'mode: ' + r.response_mode +
      ((r.rejected_fields||[]).length ? ' · model tried to smuggle: ' + r.rejected_fields.join(', ') + ' (rejected)' : '');
    document.getElementById('m-latency').textContent = 'latency ' + r.latency_ms + 'ms';
    document.getElementById('m-turns').textContent = 'turns ' + turns;
    document.getElementById('m-mode').textContent = 'mode ' + r.response_mode;
    document.getElementById('m-persona').textContent = 'persona ' + (r.persona || 'indy');
    stage('response','done'); stage('receipt','done');
    if (r.avatar && r.avatar.visemes) playVisemes(r.avatar.visemes);
    // Theater first: beats, gaze, connective tissue — then the reply itself.
    if (r.theater && r.theater.length) await playTheater(r.theater);
    speak(r.reply);
    pollReceipts();
  } catch (e) {
    addMsg('INDY', '[system] ' + e.message);
    allStages();
  }
}

function currentPersona() {
  const el = document.getElementById('persona');
  return el ? el.value : 'indy';
}


// ---------- AVATAR: deterministic puppet, graph-driven ----------
const MOUTHS = {
  X: '<path d="M84,146 Q100,150 116,144" fill="none" stroke="#7a2e18" stroke-width="3" stroke-linecap="round"/>',
  A: '<path d="M84,146 L116,146" stroke="#3d1608" stroke-width="4" stroke-linecap="round"/>',
  B: '<rect x="84" y="140" width="32" height="10" rx="2" fill="#fff" stroke="#3d1608" stroke-width="2"/><line x1="92" y1="140" x2="92" y2="150" stroke="#3d1608" stroke-width="1"/><line x1="100" y1="140" x2="100" y2="150" stroke="#3d1608" stroke-width="1"/><line x1="108" y1="140" x2="108" y2="150" stroke="#3d1608" stroke-width="1"/>',
  C: '<path d="M78,144 Q100,152 122,144" fill="none" stroke="#3d1608" stroke-width="3" stroke-linecap="round"/>',
  D: '<ellipse cx="100" cy="148" rx="12" ry="10" fill="#5c1f1f" stroke="#3d1608" stroke-width="2"/>',
  E: '<ellipse cx="100" cy="148" rx="7" ry="9" fill="#5c1f1f" stroke="#3d1608" stroke-width="2"/>',
  F: '<ellipse cx="100" cy="147" rx="10" ry="6" fill="#5c1f1f" stroke="#3d1608" stroke-width="2"/>',
  G: '<rect x="82" y="138" width="36" height="14" rx="3" fill="#fff" stroke="#3d1608" stroke-width="2"/><line x1="91" y1="138" x2="91" y2="152" stroke="#3d1608" stroke-width="1"/><line x1="100" y1="138" x2="100" y2="152" stroke="#3d1608" stroke-width="1"/><line x1="109" y1="138" x2="109" y2="152" stroke="#3d1608" stroke-width="1"/>',
  H: '<ellipse cx="100" cy="148" rx="9" ry="11" fill="#5c1f1f" stroke="#3d1608" stroke-width="2"/>',
};
const ASCII_MOUTH = { X:'___', A:'---', B:'|_|', C:'\___/', D:'(o)', E:'( )', F:'(_)', G:'|___|', H:'(O)' };
const OVERLAY_COLORS = { UNKNOWN:'#8a6d1f', PROVEN:'#1f7a3d', DISPROVEN:'#a02c2c', HYPOTHESIS:'#4a6fa5' };
let visemeTimers = [];
let currentEmotion = 'Neutral';

function setMouth(v) {
  const m = document.getElementById('mouth');
  if (!m) return;
  const tmp = document.createElementNS('http://www.w3.org/2000/svg','g');
  const key = Object.prototype.hasOwnProperty.call(MOUTHS, v) ? v : 'X';
  tmp.innerHTML = MOUTHS[key];
  const node = tmp.firstChild;
  m.replaceWith(node);
  node.id = 'mouth';
}

function setEyes(mode) {
  // mode: open | half | closed | wide | skeptical | happy
  const conf = {
    open:      { ry: 6,  pl: '76',  pr: '124' },
    half:      { ry: 3,  pl: '76',  pr: '124' },
    closed:    { ry: 0.5, pl: '76', pr: '124' },
    wide:      { ry: 8,  pl: '76',  pr: '124' },
    skeptical: { ryL: 3, ryR: 6, pl: '76', pr: '124' },
    happy:     { ry: 5,  pl: '76',  pr: '124' },
  }[mode] || { ry: 6, pl: '76', pr: '124' };
  for (const [id, cx, ry] of [['eye-l', 76, conf.ryL ?? conf.ry], ['eye-r', 124, conf.ryR ?? conf.ry]]) {
    const g = document.getElementById(id);
    if (!g) continue;
    const e = g.querySelector('ellipse');
    const r = Math.max(ry, 0.5);
    e.setAttribute('ry', r);
    e.setAttribute('cy', 102 + (6 - Math.min(ry, 6)) * 0.8);
    // hide the pupil when the eye is (nearly) shut — no floating dots
    const pupil = g.querySelector('circle');
    if (pupil) pupil.setAttribute('opacity', r < 2 ? '0' : '1');
  }
}

function setBrows(mode) {
  // mode: neutral | raised | skeptical | concerned
  const bl = document.getElementById('brow-l'), br = document.getElementById('brow-r');
  if (!bl || !br) return;
  const set = (el, x1, y1, x2, y2) => { el.setAttribute('x1',x1); el.setAttribute('y1',y1); el.setAttribute('x2',x2); el.setAttribute('y2',y2); };
  if (mode === 'raised')       { set(bl, 62,76, 90,74); set(br, 110,74, 138,76); }
  else if (mode === 'skeptical'){ set(bl, 62,74, 90,68); set(br, 110,80, 138,82); }
  else if (mode === 'concerned'){ set(bl, 62,86, 90,78); set(br, 110,78, 138,86); }
  else                         { set(bl, 62,82, 90,80); set(br, 110,80, 138,82); }
}

const EXPR_PORTRAITS = {
  Skeptical: ['art/indy/expr-skeptical.webp', 'SKEPTICAL · CLAIM UNDER REVIEW'],
  Curious:   ['art/indy/expr-thinking.webp',  'THINKING · WORKING THE ANGLES'],
  Concerned: ['art/indy/expr-thinking.webp',  'THINKING · DOESN\u2019T ADD UP'],
  Surprised: ['art/indy/expr-thinking.webp',  'THINKING · WAIT, WHAT?'],
  Delighted: ['art/indy/expr-point.webp',     'DISCOVERY · GOT IT'],
};
const EXPR_NEUTRAL = ['art/indy-portrait.webp', 'FIELD PHOTO · D. REYES, CONSULTING INVESTIGATOR'];

function setExpressionPortrait(emotion) {
  const img = document.getElementById('expr-portrait');
  const cap = document.getElementById('expr-caption');
  if (!img) return;
  // __proto__/constructor keys must not reach the map: they return
  // Object.prototype (truthy, non-iterable) and would crash destructuring.
  const hit = Object.prototype.hasOwnProperty.call(EXPR_PORTRAITS, emotion)
    ? EXPR_PORTRAITS[emotion] : EXPR_NEUTRAL;
  const [src, text] = hit;
  if (img.getAttribute('src') !== src) img.setAttribute('src', src);
  if (cap && cap.textContent !== text) cap.textContent = text;
}

function setEmotion(emotion) {
  currentEmotion = emotion;
  setExpressionPortrait(emotion);
  const label = document.getElementById('emotion-label');
  if (label) label.textContent = emotion.toLowerCase() + ' · ' + (document.getElementById('persona')?.value || 'indy');
  if (emotion === 'Curious')        { setEyes('wide'); setBrows('raised'); }
  else if (emotion === 'Skeptical'){ setEyes('skeptical'); setBrows('skeptical'); }
  else if (emotion === 'Concerned'){ setEyes('half'); setBrows('concerned'); }
  else if (emotion === 'Surprised'){ setEyes('wide'); setBrows('raised'); }
  else if (emotion === 'Delighted'){ setEyes('happy'); setBrows('raised'); }
  else                             { setEyes('open'); setBrows('neutral'); }
  drawAscii('X');
}

function setOverlay(state) {
  const bg = document.getElementById('overlay-bg'), tx = document.getElementById('overlay-text');
  if (!bg || !tx) return;
  bg.setAttribute('fill', OVERLAY_COLORS[state] || '#555');
  tx.textContent = state || '—';
}

function drawAscii(viseme) {
  const el = document.getElementById('ascii');
  if (!el) return;
  const mouth = ASCII_MOUTH[viseme] || '___';
  const eye = currentEmotion === 'Skeptical' ? '-   o' : 'o   o';
  el.textContent =
    '      .-""""-.\n' +
    "   .-'  " + eye + "  '-.\n" +
    "  |      ^       |\n" +
    '  |   .' + mouth + '.    |\n' +
    '   \   ‾‾‾‾‾‾   /\n' +
    "    '.       .'\n" +
    '      ‾‾‾‾‾‾‾\n' +
    ' [' + (document.getElementById('overlay-text')?.textContent || '?') + '] ' + currentEmotion.toLowerCase();
}

function playVisemes(timeline) {
  stopVisemes();
  if (!timeline || !timeline.length) return;
  lastVisemes = timeline;
  document.getElementById('puppet')?.classList.add('speaking');
  for (const [t, v] of timeline) {
    visemeTimers.push(setTimeout(() => { setMouth(v); drawAscii(v); }, t));
  }
  const total = timeline[timeline.length - 1][0] + 300;
  visemeTimers.push(setTimeout(() => {
    restMouth();
    document.getElementById('puppet')?.classList.remove('speaking');
  }, total));
}

function stopVisemes() {
  visemeTimers.forEach(clearTimeout);
  visemeTimers = [];
  const p = document.getElementById('puppet');
  if (p) { p.classList.remove('speaking'); p.style.transform = ''; }
}

// idle blink — the puppet is alive even when silent.
// Suppressed during utterance-start readiness and thinking pauses.
setInterval(() => {
  if (document.hidden) return;
  if (Date.now() < blinkSuppressedUntil) return;
  const prev = currentEmotion;
  setEyes('closed');
  setTimeout(() => setEmotion(prev), 150);
}, 3800);

// ---------- THEATER: the browser performs the cues, in order ----------
// The pause is not dead time. Beats, gaze shifts, and connective tissue
// choreograph what the fast lane already decided while the reply lands.
function sleep(ms) { return new Promise(function (res) { setTimeout(res, ms); }); }

function speakLine(line) {
  return new Promise(function (resolve) {
    try {
      var u = new SpeechSynthesisUtterance(line);
      u.rate = (lastVoice && lastVoice.rate) || 1.0;
      u.pitch = (lastVoice && lastVoice.pitch) || 1.0;
      u.onend = resolve; u.onerror = resolve;
      speechSynthesis.speak(u);
    } catch (e) { resolve(); }
  });
}

// ---- deterministic theater cues: gaze, blink, tilt ----
// All driven by real governor/voice events (utterance start, Emphasis,
// pause_ms). Never timers alone, never faked.
let blinkSuppressedUntil = 0;
let lastVisemes = null;

// (1) utterance-start readiness: gaze shifts to viewer, blink suppressed 150ms
function utteranceStartCue() {
  blinkSuppressedUntil = Date.now() + 150;
  setGaze('viewer');
  const p = document.getElementById('puppet');
  if (p) {
    p.style.transition = 'transform 0.25s ease';
    p.style.transform = 'translateY(2px) scale(1.01)';
    // Clear only our own lean-in; never wipe a tilt set later by thinkingCue.
    setTimeout(() => {
      const q = document.getElementById('puppet');
      if (q && q.style.transform === 'translateY(2px) scale(1.01)') q.style.transform = '';
    }, 600);
  }
}

// (2) emphasis moment: both brows raise +8deg at the stressed viseme,
// synced to the real viseme timeline, then restore the emotion's brows
function emphasisCue(timeline, emphasis) {
  if (!emphasis || emphasis === 'Neutral' || !timeline || !timeline.length) return;
  const mid = timeline[Math.floor(timeline.length / 2)][0];
  visemeTimers.push(setTimeout(() => {
    setBrows('raised');
    visemeTimers.push(setTimeout(() => setEmotion(currentEmotion), 450));
  }, mid));
}

// (3) pause > 200ms "thinking" signal: head-tilt -5deg, slow blink, gaze avert
function thinkingCue(pauseMs) {
  const p = document.getElementById('puppet');
  if (p) {
    p.classList.remove('speaking'); // pause the head-bob so the tilt reads
    p.style.transition = 'transform 0.3s ease';
    p.style.transform = 'rotate(-5deg)';
  }
  setGaze('away');
  blinkSuppressedUntil = Date.now() + pauseMs;
  setEyes('closed');                       // slow blink: shut 120ms...
  setTimeout(() => setEyes('half'), 120);  // ...half 120ms...
  setTimeout(() => setEmotion(currentEmotion), 260); // ...then restore
}

function setGaze(target) {
  // Pupils drift toward the evidence, away while "retrieving",
  // or toward the viewer at utterance start (slight downward look-at-you).
  var dx = target === 'evidence' ? 3 : target === 'away' ? -4 : 0;
  var dy = target === 'viewer' ? 1.5 : 0;
  ['pupil-l', 'pupil-r'].forEach(function (id) {
    var p = document.getElementById(id);
    if (p) { p.setAttribute('cx', (id === 'pupil-l' ? 76 : 124) + dx); p.setAttribute('cy', 102 + dy); }
  });
  setTimeout(function () {
    ['pupil-l', 'pupil-r'].forEach(function (id) {
      var p = document.getElementById(id);
      if (p) { p.setAttribute('cx', id === 'pupil-l' ? 76 : 124); p.setAttribute('cy', 102); }
    });
  }, 1600);
}

function theaterGesture(name) {
  var puppet = document.getElementById('puppet');
  if (!puppet) return;
  // notebook: a small tilt, as if jotting something down. nod: one dip.
  puppet.style.transition = 'transform 0.35s ease';
  puppet.style.transform = name === 'notebook' ? 'rotate(-4deg) translateY(3px)' : 'translateY(4px)';
  setTimeout(function () { puppet.style.transform = ''; }, 700);
}

function theaterReveal(what) {
  var el = document.getElementById(what === 'receipt' ? 'receiptlog' : 'evidence');
  if (!el) el = document.getElementById('receiptlog');
  if (!el) return;
  el.style.transition = 'box-shadow 0.4s ease';
  el.style.boxShadow = '0 0 12px 2px rgba(255,210,120,0.65)';
  setTimeout(function () { el.style.boxShadow = ''; }, 1200);
}

async function playTheater(cues) {
  for (var i = 0; i < (cues || []).length; i++) {
    var c = cues[i];
    if (c.cue === 'beat') await sleep(Math.min(c.ms, 1200));
    else if (c.cue === 'speak') await speakLine(c.line);
    else if (c.cue === 'gaze') setGaze(c.target);
    else if (c.cue === 'gesture') theaterGesture(c.name);
    else if (c.cue === 'reveal') theaterReveal(c.what);
    else if (c.cue === 'energy' && lastVoice) {
      lastVoice.rate = Math.max(0.5, Math.min(2.0, lastVoice.rate * (1 + c.delta)));
    }
  }
  setGaze('center');
}

// Voice is a graph projection: the governor's VoiceState drives real TTS
// prosody. Skepticism is slower, lower, with a beat before the key clause.
let lastVoice = null;
function restMouth() {
  // Skepticism flattens the smirk; everything else settles to rest.
  setMouth(currentEmotion === 'Skeptical' ? 'A' : 'X');
  drawAscii(currentEmotion === 'Skeptical' ? 'A' : 'X');
}

function speak(text) {
  try {
    const clean = text.replace(/\[fixture[^\]]*\]\s*/, '').slice(0, 400);
    const u = new SpeechSynthesisUtterance(clean);
    // Doubt pauses: honor pause_ms before the utterance.
    const pause = Math.min((lastVoice && lastVoice.pause_ms) || 0, 1200);
    // Theater cues, all driven by real governor/voice data:
    utteranceStartCue();                                            // (1) readiness
    emphasisCue(lastVisemes, lastVoice && lastVoice.emphasis);      // (2) stress
    if (lastVoice) {
      u.rate = lastVoice.rate || 1.0;
      u.pitch = lastVoice.pitch || 1.0;
    } else {
      u.rate = 1.05;
    }
    // If speech ends early, settle the puppet — don't leave it mid-viseme.
    u.onend = () => { stopVisemes(); restMouth(); };
    u.onerror = () => { stopVisemes(); restMouth(); };
    speechSynthesis.cancel();
    const doSpeak = () => {
      document.getElementById('puppet')?.classList.add('speaking');
      const p = document.getElementById('puppet');
      if (p && p.style.transform === 'rotate(-5deg)') p.style.transform = '';
      speechSynthesis.speak(u);
    };
    if (pause > 200) { thinkingCue(pause); setTimeout(doSpeak, pause); }  // (3)
    else if (pause > 0) setTimeout(doSpeak, pause);
    else doSpeak();
  } catch (e) { /* TTS failed visibly: no speech, chat still works */ }
}

// Mic: Web Speech API, push-to-talk style.
const micBtn = document.getElementById('mic');
let rec = null, listening = false;
micBtn.onclick = () => {
  const SR = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!SR) { alert('Speech recognition not available — type instead.'); return; }
  if (listening) { rec.stop(); return; }
  rec = new SR(); rec.lang = 'en-US'; rec.interimResults = false;
  rec.onresult = e => {
    const t = e.results[0][0].transcript;
    document.getElementById('text').value = '';
    sendTurn(t);
  };
  rec.onend = () => { listening = false; micBtn.classList.remove('on'); micBtn.textContent = '🎙'; };
  rec.onerror = e => { addMsg('INDY', '[voice input failed: ' + e.error + '] — type instead.'); };
  rec.start();
  listening = true; micBtn.classList.add('on'); micBtn.textContent = '⏹';
};
document.getElementById('send').onclick = () => {
  const t = document.getElementById('text').value;
  document.getElementById('text').value = '';
  sendTurn(t);
};
document.getElementById('text').addEventListener('keydown', e => {
  if (e.key === 'Enter') document.getElementById('send').click();
});

// CORRECTION flash: visible, transient, driven only by a real revision
// receipt in the turn response. Never shown on a timer alone.
function flashCorrection(rev) {
  let el = document.getElementById('correction-flash');
  if (!el) {
    el = document.createElement('div');
    el.id = 'correction-flash';
    el.style.cssText = 'position:fixed;top:12px;left:50%;transform:translateX(-50%);' +
      'background:#3a2b00;color:#ffd866;border:1px solid #ffd866;border-radius:8px;' +
      'padding:8px 14px;font-weight:bold;z-index:9999;box-shadow:0 2px 12px rgba(0,0,0,.4);';
    document.body.appendChild(el);
  }
  const detail = rev ? (' ' + String(rev.claim || '') + ': ' +
    String(rev.old_value || '') + ' → ' + String(rev.new_value || '')) : '';
  el.textContent = '⚡ CORRECTION DETECTED — speech stopped, claim revised.' + detail;
  el.style.display = 'block';
  clearTimeout(flashCorrection._t);
  flashCorrection._t = setTimeout(() => { el.style.display = 'none'; }, 4000);
}

async function pollReceipts() {
  try {
    const res = await fetch(api('/api/receipts'));
    if (!res.ok) throw new Error('no governor');
    const rs = await res.json();
    const log = document.getElementById('receiptlog');
    log.innerHTML = '';
    rs.slice(0, 12).forEach(r => {
      const d = document.createElement('div');
      d.className = 'r';
      // CORRECTION receipts: render the old -> new revision, not the
      // generic decision line. Detail comes from the backend's structured
      // ClaimRevision field; text nodes only, never innerHTML.
      if (r.operation === 'claim.revise' && r.revision) {
        d.className = 'r r-correction';
        const t = new Date(r.timestamp_ms).toLocaleTimeString();
        const b = document.createElement('b'); b.textContent = t; d.appendChild(b);
        d.appendChild(document.createTextNode(' ⚡ CORRECTION ' +
          String(r.revision.claim || '') + ': ' +
          String(r.revision.old_value || '') + ' → ' +
          String(r.revision.new_value || '') +
          ' · ' + String(r.revision.previous_output || '') +
          ' · speech_interrupted ' + String(r.revision.speech_interrupted)));
        d.title = 'claim revision receipt ' + r.receipt_id;
        log.appendChild(d);
        return;
      }
      const t = new Date(r.timestamp_ms).toLocaleTimeString();
      const dec = String(r.decision || '').toUpperCase();
      const cls = dec === 'ALLOW' ? 'dec-allow' : dec === 'BLOCK' ? 'dec-block' : '';
      // Latency ribbon: honest bands from the receipt's own measured duration.
      const ms = Number(r.duration_ms) || 0;
      const band = ms <= 200 ? 'fast' : ms <= 500 ? 'mid' : 'slow';
      const w = Math.max(6, Math.min(72, Math.round(ms / 10)));
      const rib = document.createElement('span');
      rib.className = 'ribbon ' + band;
      rib.style.width = w + 'px';
      rib.title = ms + 'ms';
      d.appendChild(rib);
      const body = document.createElement('span');
      // Receipt fields come from the backend (possibly a hostile ?api=
      // override): build with text nodes only, never innerHTML.
      const tEl = document.createElement('b'); tEl.textContent = t; body.appendChild(tEl);
      body.appendChild(document.createTextNode(' ' + String(r.operation || '') + ' → '));
      const decEl = document.createElement('span'); decEl.className = cls; decEl.textContent = dec; body.appendChild(decEl);
      body.appendChild(document.createTextNode(' ' + String(r.epistemic_status || '') + ' '));
      const msEl = document.createElement('span'); msEl.textContent = ms + 'ms'; body.appendChild(msEl);
      body.appendChild(document.createTextNode(' '));
      const idEl = document.createElement('span'); idEl.style.color = '#444';
      idEl.textContent = String(r.receipt_id || '').slice(0, 8); body.appendChild(idEl);
      d.appendChild(body);
      d.title = 'receipt ' + r.receipt_id + (r.prev_hash ? ' · prev ' + String(r.prev_hash).slice(0,8) : '');
      log.appendChild(d);
    });
  } catch (e) {
    // Static deploy: no governor, no receipts. Say so once, stop polling.
    const log = document.getElementById('receiptlog');
    if (log && !log.dataset.static) {
      log.dataset.static = '1';
      log.innerHTML = '<div class="r empty">no receipts — this static page has no governor. Run locally or open with ?api= pointing at one.</div>';
    }
    if (receiptTimer) { clearInterval(receiptTimer); receiptTimer = null; }
  }
}
let receiptTimer = setInterval(pollReceipts, 2000);
pollReceipts();

// WebSocket live receipt stream (falls back to polling silently).
try {
  const ws = new WebSocket((location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host + '/ws');
  ws.onmessage = () => pollReceipts();
} catch (e) {}

// avatar boot: neutral, at rest, UNKNOWN pending
setEmotion('Neutral');
setOverlay('UNKNOWN');
drawAscii('X');

// Honest connection badge: probe the governor. Green only if it answers;
// the static deploy must never claim to be live.
(async function probeGovernor() {
  const el = document.getElementById('conn');
  try {
    const ctl = new AbortController();
    const to = setTimeout(() => ctl.abort(), 4000);
    const res = await fetch(api('/api/health'), { signal: ctl.signal });
    clearTimeout(to);
    if (res.ok) { el.textContent = '● LIVE'; el.className = 'conn live'; el.title = 'governor reachable'; return; }
    throw new Error('unhealthy');
  } catch (e) {
    el.textContent = '○ STATIC — no governor';
    el.className = 'conn static';
    el.title = 'UI only. Point at a governor with ?api= or run locally.';
  }
})();

// Selector feedback: costume changes say what they change — and what they don't.
let toastTimer = null;
function selectToast(msg) {
  const t = document.getElementById('selectoast');
  if (!t) return;
  t.textContent = msg;
  t.classList.add('on');
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove('on'), 2600);
}
[['persona', v => 'costume → ' + v + ' · voice + face change · governor unchanged'],
 ['era', v => 'era → ' + v + ' · voice instrument changes · verdicts do not'],
 ['tone', v => 'tone → ' + v + ' · delivery color only · epistemics untouched']
].forEach(([id, msg]) => {
  const el = document.getElementById(id);
  if (el) el.addEventListener('change', () => selectToast(msg(el.value)));
});

// One-tap demo: the two killer utterances, real requests, real responses.
document.querySelectorAll('button.demo').forEach(b => {
  b.addEventListener('click', () => {
    const input = document.getElementById('text');
    if (input) input.value = b.dataset.say || '';
    sendTurn(b.dataset.say || '');
  });
});
