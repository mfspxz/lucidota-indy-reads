// Indy_Reads LIVE voice client (Track A).
// Browser --WS--> Rust gateway --WS--> Alebex engine. The engine token NEVER
// reaches this file: the browser only ever sees an ephemeral session id.
// Mic: PCM16 16kHz mono, 20ms frames (640 bytes). Agent audio: PCM16 24kHz.
// Mark semantics: the gateway HOLDS engine marks; this client echoes a mark
// only after its audio queue fully drains. clear_audio kills the queue.

(function () {
  'use strict';
  var ws = null, sessionId = null, micStream = null, micCtx = null, micNode = null;
  var _apiRaw = '';
  try { _apiRaw = (new URLSearchParams(location.search).get('api') || '').replace(/\/$/, ''); } catch (e) {}
  // Same rule as app.js: ?api= points at an http(s) governor, nothing else.
  var API_BASE = (/^https?:\/\//i.test(_apiRaw) || _apiRaw === '') ? _apiRaw : '';
  // ?key= gate (RT-SWEEP-2026-10-08-003): the API requires the inbound key.
  // Read once from the page URL and append to every API call — never hardcoded.
  var _inboundKey = '';
  try { _inboundKey = new URLSearchParams(location.search).get('key') || ''; } catch (e) {}
  function withKey(url) {
    if (!_inboundKey) return url;
    return url + (url.indexOf('?') >= 0 ? '&' : '?') + 'key=' + encodeURIComponent(_inboundKey);
  }
  var playCtx = null, playQueue = [], playing = false, pendingMarks = [];
  var liveBtn = null, statusEl = null;

  function log(who, text) {
    if (typeof addMsg === 'function') addMsg(who, text);
    else console.log('[' + who + ']', text);
  }

  function setStatus(s) {
    if (statusEl) statusEl.textContent = s;
  }

  var WORKLET = `
    class MicCap extends AudioWorkletProcessor {
      constructor() { super(); this.buf = []; }
      process(inputs) {
        var ch = inputs[0] && inputs[0][0];
        if (!ch) return true;
        for (var i = 0; i < ch.length; i++) this.buf.push(ch[i]);
        while (this.buf.length >= 320) {
          var frame = this.buf.splice(0, 320);
          var pcm = new Int16Array(320);
          for (var j = 0; j < 320; j++) {
            var s = Math.max(-1, Math.min(1, frame[j]));
            pcm[j] = Math.round(s * 32767);
          }
          this.port.postMessage(pcm.buffer, [pcm.buffer]);
        }
        return true;
      }
    }
    registerProcessor('mic-cap', MicCap);
  `;

  function ensurePlayCtx() {
    if (!playCtx) playCtx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 24000 });
    if (playCtx.state === 'suspended') playCtx.resume();
    return playCtx;
  }

  function playPcm16(ab) {
    // Belt and braces with the gateway's 256KB cap: never let a rogue
    // engine OOM the tab through the playback queue.
    if (ab.byteLength > 262144 || playQueue.length > 200) return;
    var ctx = ensurePlayCtx();
    var i16 = new Int16Array(ab);
    var buf = ctx.createBuffer(1, i16.length, 24000);
    var f32 = buf.getChannelData(0);
    for (var i = 0; i < i16.length; i++) f32[i] = i16[i] / 32768;
    playQueue.push(buf);
    pump();
  }

  function pump() {
    if (playing || !playQueue.length) return;
    playing = true;
    var ctx = ensurePlayCtx();
    var src = ctx.createBufferSource();
    src.buffer = playQueue.shift();
    src.connect(ctx.destination);
    src.onended = function () {
      playing = false;
      if (!playQueue.length) onDrained();
      pump();
    };
    src.start();
  }

  // Audio queue fully drained: every held mark may now be echoed, in order.
  // A single slot would silently drop all but the last mark when several
  // arrive while audio is still playing. Never echo before drain.
  function onDrained() {
    while (pendingMarks.length && ws && ws.readyState === 1) {
      ws.send(JSON.stringify({ type: 'mark_echo', id: pendingMarks.shift() }));
    }
  }

  function clearAudio() {
    playQueue = [];
    playing = false;
    pendingMarks = [];
    if (playCtx) { try { playCtx.close(); } catch (e) {} playCtx = null; }
  }

  // FAST LANE: deterministic correction detection on the transcript path.
  // String patterns only — never model inference. Mirrors the server-side
  // detect_correction() in api.rs so both lanes agree on what a correction is.
  function parseCorrection(text) {
    var lower = String(text).toLowerCase();
    var pats = ['no, sorry', 'no sorry', 'i meant', 'correction:', 'correction -'];
    for (var i = 0; i < pats.length; i++) {
      var idx = lower.indexOf(pats[i]);
      if (idx >= 0) {
        var rest = String(text).slice(idx + pats[i].length).replace(/^[\s,:-]+/, '').trim().slice(0, 120);
        return { claim: 'active_claim', old_value: 'unspecified', new_value: rest || 'unspecified' };
      }
    }
    var m = lower.match(/^actually[,\s]+(.*)/);
    if (m) {
      var rest2 = (m[1] || '').trim().slice(0, 120);
      return { claim: 'active_claim', old_value: 'unspecified', new_value: rest2 || 'unspecified' };
    }
    return null;
  }

  // Visible retraction flash. Shown only on a real detected correction.
  function flashCorrectionLive(corr) {
    var el = document.getElementById('correction-flash-live');
    if (!el) {
      el = document.createElement('div');
      el.id = 'correction-flash-live';
      el.style.cssText = 'position:fixed;top:12px;left:50%;transform:translateX(-50%);' +
        'background:#3a2b00;color:#ffd866;border:1px solid #ffd866;border-radius:8px;' +
        'padding:8px 14px;font-weight:bold;z-index:9999;box-shadow:0 2px 12px rgba(0,0,0,.4);';
      document.body.appendChild(el);
    }
    el.textContent = '⚡ CORRECTION DETECTED — speech stopped, revising claim…';
    el.style.display = 'block';
    clearTimeout(flashCorrectionLive._t);
    flashCorrectionLive._t = setTimeout(function () { el.style.display = 'none'; }, 4000);
  }

  // The correction turn: declared explicitly so the governor emits the
  // ClaimRevision receipt and invalidates pending output.
  function sendCorrectionTurn(text, corr) {
    fetch(API_BASE + '/api/turn', {
      method: 'POST', headers: { 'Content-Type': 'application/json',
        'Authorization': 'Bearer ' + _inboundKey },
      body: JSON.stringify({ utterance: text, correction_of: corr })
    }).then(function (res) { return res.json(); }).then(function (r) {
      log('INDY', '[correction] ' + r.reply +
        (r.speech_interrupted ? ' [speech interrupted — claim revised]' : ''));
      if (typeof pollReceipts === 'function') pollReceipts();
    }).catch(function () { /* gateway unreachable: the typed path still works */ });
  }

  // Detection-to-interruption: on an INTERIM transcript matching a
  // correction pattern, kill the audio queue immediately — before the slow
  // lane has done anything. The final transcript then files the revision.
  function fastLaneCorrection(text, isFinal) {
    var corr = parseCorrection(text);
    if (!corr) return false;
    var audioLive = playing || playQueue.length > 0;
    if (audioLive || !isFinal) {
      // Interim match: stop speech NOW. The receipt comes with the final turn.
      if (audioLive) {
        clearAudio();
        flashCorrectionLive(corr);
        log('INDY', '[live] correction detected mid-speech — audio stopped.');
      }
    }
    if (isFinal) {
      sendCorrectionTurn(text, corr);
    }
    return true;
  }

  async function startMic() {
    micStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    micCtx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 16000 });
    var blob = new Blob([WORKLET], { type: 'application/javascript' });
    await micCtx.audioWorklet.addModule(URL.createObjectURL(blob));
    var src = micCtx.createMediaStreamSource(micStream);
    micNode = new AudioWorkletNode(micCtx, 'mic-cap');
    micNode.port.onmessage = function (e) {
      if (ws && ws.readyState === 1 && e.data.byteLength === 640) ws.send(e.data);
    };
    src.connect(micNode);
  }

  function stopMic() {
    if (micNode) { try { micNode.disconnect(); } catch (e) {} micNode = null; }
    if (micCtx) { try { micCtx.close(); } catch (e) {} micCtx = null; }
    if (micStream) { micStream.getTracks().forEach(function (t) { t.stop(); }); micStream = null; }
  }

  async function startCall() {
    if (ws) { endCall(); return; }
    setStatus('requesting session…');
    var res;
    try {
      res = await fetch(API_BASE + '/api/alebex/session', {
        headers: { 'Authorization': 'Bearer ' + _inboundKey } });
    } catch (e) { setStatus('gateway unreachable'); return; }
    if (!res.ok) {
      var msg = await res.text();
      setStatus('live voice unavailable: ' + msg);
      log('INDY', '[live] voice session unavailable (' + res.status + '): ' + msg +
        ' — typed voice still works. The gateway fails closed without engine credentials.');
      return;
    }
    var info = await res.json();
    sessionId = info.session_id;
    // ws_url must be a same-host path. A hostile ?api= backend could
    // otherwise redirect the socket (and the mic) to an arbitrary host.
    if (typeof info.ws_url !== 'string' || info.ws_url.charAt(0) !== '/') {
      setStatus('live voice unavailable: bad gateway response');
      log('INDY', '[live] gateway returned an invalid socket path — refusing to connect.');
      return;
    }
    var proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    var host = location.host;
    if (API_BASE) { host = API_BASE.replace(/^https?:\/\//, ''); proto = API_BASE.indexOf('https:') === 0 ? 'wss:' : 'ws:'; }
    ws = new WebSocket(proto + '//' + host + info.ws_url);
    ws.binaryType = 'arraybuffer';
    ws.onopen = async function () {
      setStatus('LIVE — speak');
      log('INDY', '[live] connected. Speak — the governor is listening.');
      try { await startMic(); } catch (e) { log('INDY', '[live] mic failed: ' + e.message); }
    };
    ws.onmessage = function (ev) {
      // Cap binary frames: a hostile gateway could blast huge buffers at the
      // audio queue (memory exhaustion). 1MB >> any legit 20ms PCM frame.
      if (ev.data instanceof ArrayBuffer) {
        if (ev.data.byteLength > 1048576) return;
        playPcm16(ev.data); return;
      }
      var m;
      try { m = JSON.parse(ev.data); } catch (e) { return; }
      if (m.type === 'transcript' && m.text) {
        // Fast lane first: a correction pattern stops speech before the
        // slow lane sees the final transcript.
        if (!fastLaneCorrection(m.text, !!m.final)) {
          if (m.final) log('YOU', '[live] ' + m.text);
        } else if (m.final) {
          log('YOU', '[live] ' + m.text);
        }
      }
      else if (m.type === 'mark_pending') {
        // A mark is waiting on drained playback. Echo now only if the queue
        // is already empty; otherwise queue it — every mark echoes in order
        // when the audio drains, none are dropped.
        if (!playQueue.length && !playing) {
          ws.send(JSON.stringify({ type: 'mark_echo', id: m.id }));
        } else {
          pendingMarks.push(m.id);
        }
      } else if (m.type === 'clear_audio') {
        clearAudio();
        log('INDY', '[live] audio cleared (barge-in).');
      } else if (m.type === 'call_ended') {
        log('INDY', '[live] call ended.');
        endCall();
      }
    };
    ws.onclose = function () { setStatus('idle'); };
    ws.onerror = function () { setStatus('connection error'); };
  }

  function endCall() {
    if (ws) {
      try { ws.send(JSON.stringify({ type: 'end_call' })); } catch (e) {}
      try { ws.close(); } catch (e) {}
      ws = null;
    }
    stopMic();
    clearAudio();
    sessionId = null;
    setStatus('idle');
    log('INDY', '[live] call closed.');
  }

  // Inject the LIVE control without touching index.html (Track B owns it).
  function mount() {
    var bar = document.querySelector('.controls') || document.body;
    liveBtn = document.createElement('button');
    liveBtn.id = 'livecall';
    liveBtn.textContent = 'LIVE';
    liveBtn.title = 'Live voice via the Alebex gateway (server-side key)';
    liveBtn.onclick = startCall;
    statusEl = document.createElement('span');
    statusEl.id = 'livestatus';
    statusEl.textContent = 'idle';
    statusEl.style.marginLeft = '8px';
    bar.appendChild(liveBtn);
    bar.appendChild(statusEl);
    // Third-party governor warning: ?api= means the LIVE path (mic audio
    // included) talks to someone else's server. Say so before they click.
    if (API_BASE) {
      var warn = document.createElement('span');
      warn.id = 'apiwarn';
      warn.textContent = '⚠ third-party governor: ' + API_BASE;
      warn.title = 'This page is pointed at a third-party governor via ?api=. ' +
        'Typed turns, receipts, and LIVE mic audio go to that server, not to us. ' +
        'Only use this with a governor you trust.';
      warn.style.marginLeft = '8px';
      warn.style.color = '#b8860b';
      warn.style.fontSize = '12px';
      bar.appendChild(warn);
    }
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mount);
  else mount();
})();
