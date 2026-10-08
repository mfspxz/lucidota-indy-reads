// INDY-VOICE guard harness — behavioral verification of web/voice.js
// self-talk spam controls with MEASURED timings (real Date.now, real timers).
//
// Loads the REAL voice.js source in a stubbed browser environment and drives
// its recognition-result path. No browser needed.
//
//   node web/voice-guard-harness.mjs
// Exit 0 = all guards verified. Exit != 0 = a guard failed (details on stdout).
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import vm from 'vm';
import path from 'path';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SRC = readFileSync(path.join(HERE, 'voice.js'), 'utf8');

// ---------- observation bus ----------
const obs = {
  fetchCalls: [],        // {t, url, body}
  speakStarts: [],       // t when speechSynthesis.speak(u) invoked
  audioStarts: [],       // t when u.onstart fired (sound actually out)
  speakEnds: [],         // t when u.onend fired
  replyN: 0,
};
const now = () => Date.now();

// ---------- DOM stubs ----------
function makeEl(id) {
  return {
    id, textContent: '', className: '', style: {},
    children: [],
    appendChild(c) { this.children.push(c); if (this.children.length > 200) this.children.shift(); },
    removeChild(c) { const i = this.children.indexOf(c); if (i >= 0) this.children.splice(i, 1); },
    get firstChild() { return this.children[0]; },
    setAttribute() {}, getAttribute() { return ''; },
    classList: { add() {}, remove() {} },
  };
}
const els = {};
const documentStub = {
  getElementById: id => (els[id] ||= makeEl(id)),
  createElement: () => makeEl('dyn'),
  body: makeEl('body'),
};
documentStub.getElementById('log').scrollTop = 0;
documentStub.getElementById('log').scrollHeight = 0;

// ---------- SpeechRecognition stub ----------
let recInstance = null;
class FakeSR {
  constructor() {
    recInstance = this;
    this.continuous = true; this.interimResults = true; this.lang = 'en-US';
  }
  start() { if (this.onstart) this.onstart(); }
  stop() { if (this.onend) this.onend(); }
  // test driver: inject a recognition result
  __fire(text, isFinal = true) {
    const e = { results: [{ 0: { transcript: text }, isFinal, length: 1 }], };
    e.results[e.results.length - 1] = { 0: { transcript: text }, isFinal };
    if (this.onresult) this.onresult(e);
  }
}

// ---------- speechSynthesis stub ----------
const synthStub = {
  _voices: [{ name: 'Google US English', lang: 'en-US' }],
  getVoices() { return this._voices; },
  cancel() {},
  speak(u) {
    obs.speakStarts.push(now());
    setTimeout(() => { if (u.onstart) { obs.audioStarts.push(now()); u.onstart(); } }, 20);
    setTimeout(() => { if (u.onend) { obs.speakEnds.push(now()); u.onend(); } }, 120);
  },
  onvoiceschanged: null,
};
function SpeechSynthesisUtterance(text) { this.text = text; }

// ---------- Audio stub ----------
class FakeAudio {
  constructor() { this._t = null; }
  play() {
    return new Promise(res => {
      this._t = setTimeout(() => { if (this.onplaying) this.onplaying(); }, 30);
      this._t2 = setTimeout(() => { if (this.onended) this.onended(); }, 150);
      res();
    });
  }
  pause() { clearTimeout(this._t); clearTimeout(this._t2); }
}

// ---------- fetch stub: /api/turn with 150ms brain latency ----------
async function fetchStub(url, opts) {
  const t = now();
  const body = opts && opts.body ? JSON.parse(opts.body) : {};
  obs.fetchCalls.push({ t, url, utterance: (body.utterance || '').slice(0, 60) });
  await new Promise(r => setTimeout(r, 150)); // simulated brain latency
  obs.replyN++;
  return {
    ok: true, status: 200,
    json: async () => ({
      reply: `fixture reply number ${obs.replyN} about ${body.utterance || 'nothing'}`.slice(0, 120),
      epistemic_status: 'UNKNOWN',
      session_id: 's-harness',
      avatar: { emotion: 'Neutral' },
      voice: { rate: 1.0, pitch: 1.0 },
    }),
  };
}

// ---------- sandbox ----------
const sandbox = {
  console, setTimeout, clearTimeout, setInterval, clearInterval, Date, JSON,
  Math, Promise, URL,
  document: documentStub,
  window: {},
  navigator: {
    userAgent: 'node-voice-guard-harness',
    mediaDevices: { getUserMedia: async () => ({ getTracks: () => [{ stop() {} }] }) },
  },
  localStorage: { _m: {}, getItem(k) { return this._m[k] ?? null; }, setItem(k, v) { this._m[k] = v; } },
  SpeechRecognition: FakeSR,
  speechSynthesis: synthStub,
  SpeechSynthesisUtterance,
  Audio: FakeAudio,
  fetch: fetchStub,
  encodeURIComponent,
  addEventListener: () => {},
};
sandbox.window = sandbox;
sandbox.globalThis = sandbox;
vm.createContext(sandbox);
vm.runInContext(SRC, sandbox, { filename: 'voice.js' });

// wait for mic grant + rec start
await new Promise(r => setTimeout(r, 300));
if (!recInstance) { console.log('FAIL: recognition instance never created'); process.exit(1); }

const results = [];
function check(name, cond, detail) {
  results.push({ name, pass: !!cond, detail: detail || '' });
  console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${detail ? ' — ' + detail : ''}`);
}
const sleep = ms => new Promise(r => setTimeout(r, ms));
const fetchCount = () => obs.fetchCalls.length;
// settle: wait until no turn in flight and no speech (poll)
// settlePastCooldown: additionally wait out the 2500ms post-speech echo
// cooldown so the next anchor is not dropped by the guard under test.
async function settlePastCooldown() {
  await settle();
  await sleep(2700);
}
// settle: wait until no turn in flight and no speech (poll)
async function settle(timeoutMs = 8000) {
  const t0 = now();
  while (now() - t0 < timeoutMs) {
    await sleep(100);
    const lastFetch = obs.fetchCalls.length ? obs.fetchCalls[obs.fetchCalls.length - 1].t : 0;
    const lastEnd = obs.speakEnds.length ? obs.speakEnds[obs.speakEnds.length - 1] : 0;
    if (obs.fetchCalls.length === 0 || (lastEnd > lastFetch && now() - lastEnd > 400)) return true;
  }
  return false;
}

// ============ T1: one turn in flight max ============
{
  const before = fetchCount();
  const t0 = now();
  for (let i = 0; i < 5; i++) recInstance.__fire(`rapid fire ${i}`, true);
  const dt = now() - t0;
  await sleep(60);
  const during = fetchCount() - before;
  check('T1 one-turn-in-flight: 5 rapid results -> exactly 1 fetch', during === 1,
    `5 results in ${dt}ms produced ${during} fetch call(s)`);
  await settle();
}

// ============ T2: echo fingerprint ============
{
  // lastSaid is now the canned reply of the last turn. Feed it back verbatim.
  const lastReply = `fixture reply number ${obs.replyN} about rapid fire 0`.slice(0, 120);
  const before = fetchCount();
  recInstance.__fire(lastReply, true);           // her own words -> dropped
  await sleep(80);
  const afterEcho = fetchCount() - before;
  recInstance.__fire('a completely different question about birds', true); // distinct -> accepted
  await sleep(80);
  const afterDistinct = fetchCount() - before;
  check('T2 echo fingerprint: own words dropped, distinct accepted',
    afterEcho === 0 && afterDistinct === 1,
    `echo->${afterEcho} fetches, distinct->${afterDistinct - afterEcho} fetch(es)`);
  // case/punctuation-insensitive variant
  const before2 = fetchCount();
  recInstance.__fire('FIXTURE REPLY NUMBER ' + obs.replyN + '!!! about A completely different question about birds', true);
  await sleep(80);
  check('T2b echo fingerprint is case/punctuation-insensitive',
    fetchCount() - before2 === 0, `garbled-echo produced ${fetchCount() - before2} fetch(es)`);
  await settlePastCooldown();
}

// ============ T3: 2500ms post-speech cooldown, MEASURED boundary ============
{
  // anchor turn -> speak -> stopVisemes sets quietUntil = speakEnd + 2500.
  // (settlePastCooldown above guarantees the anchor is outside any prior
  // cooldown, so a missing speak-end here is a REAL failure, not an artifact.)
  const speakEndBefore = obs.speakEnds.length;
  recInstance.__fire('cooldown anchor turn', true);
  // wait for this turn's speech to end
  let guard = 0;
  while (obs.speakEnds.length === speakEndBefore && guard++ < 100) await sleep(100);
  const tSpeakEnd = obs.speakEnds[obs.speakEnds.length - 1];
  check('T3 setup: anchor speech ended', obs.speakEnds.length > speakEndBefore,
    `speakEnd at t=${tSpeakEnd}`);

  // binary search the drop/accept boundary with fresh anchors each probe
  // Each probe is strictly serialized: idle + past cooldown, then an
  // anchor turn whose acceptance AND speech-end are both verified before
  // t0 is taken. A dropped anchor invalidates the probe (retried once).
  async function probe(delayMs, tag) {
    for (let attempt = 0; attempt < 2; attempt++) {
      await settlePastCooldown();
      const f0 = fetchCount(), se = obs.speakEnds.length;
      recInstance.__fire(`anchor ${tag} try${attempt}`, true);
      let g = 0;
      while (fetchCount() === f0 && g++ < 100) await sleep(50);
      if (fetchCount() !== f0 + 1) continue; // anchor dropped; retry
      g = 0;
      while (obs.speakEnds.length === se && g++ < 150) await sleep(100);
      if (obs.speakEnds.length !== se + 1) continue; // no speech; retry
      const t0 = obs.speakEnds[obs.speakEnds.length - 1];
      const waitMs = t0 + delayMs - now();
      if (waitMs > 0) await sleep(waitMs);
      const before = fetchCount();
      recInstance.__fire(`distinct probe ${tag} ${delayMs}ms`, true);
      await sleep(150);
      return { accepted: fetchCount() - before > 0, t0, valid: true };
    }
    return { accepted: false, t0: 0, valid: false };
  }
  const r2400 = await probe(2400, 'a');
  const r2600 = await probe(2600, 'b');
  check('T3 probes valid (anchors accepted)', r2400.valid && r2600.valid,
    `a.valid=${r2400.valid} b.valid=${r2600.valid}`);
  let lo = 2400, hi = 2600, blo = r2400.accepted, bhi = r2600.accepted;
  // bisect 4 rounds -> ~12ms resolution
  for (let i = 0; i < 4; i++) {
    const mid = Math.round((lo + hi) / 2);
    const r = await probe(mid, 'm' + i);
    if (r.accepted) { hi = mid; bhi = true; } else { lo = mid; blo = false; }
  }
  const measured = Math.round((lo + hi) / 2);
  check('T3 cooldown: dropped at 2400ms, accepted at 2600ms',
    !r2400.accepted && r2600.accepted, `2400ms->${r2400.accepted ? 'accepted' : 'dropped'}, 2600ms->${r2600.accepted ? 'accepted' : 'dropped'}`);
  check('T3b measured cooldown boundary ~= 2500ms', Math.abs(measured - 2500) <= 60,
    `measured boundary ~${measured}ms (bisection [${lo},${hi}])`);
  await settle();
}

// ============ T4: fingerprint armed BEFORE audio ============
{
  // Start a turn; when speak() is invoked (before onstart/audio), fire the
  // new reply text as a mic result. It must be dropped: lastSaid is set
  // synchronously in speak(), ahead of any sound.
  await settlePastCooldown();
  const before = fetchCount();
  const n0 = obs.replyN;
  const spkBefore = obs.speakStarts.length;
  recInstance.__fire('fingerprint timing probe', true);
  // the probe turn itself must start (else the measurement is meaningless)
  let g0 = 0;
  while (fetchCount() === before && g0++ < 100) await sleep(50);
  const probeStarted = fetchCount() === before + 1;
  let g = 0;
  while (obs.speakStarts.length === spkBefore && g++ < 200) await sleep(20);
  const speakStarted = obs.speakStarts.length === spkBefore + 1;
  const expectedReply = `fixture reply number ${n0 + 1} about fingerprint timing probe`.slice(0, 120);
  // speak() has been called (lastSaid armed), audio not yet started (onstart +20ms)
  recInstance.__fire(expectedReply, true);
  await sleep(80);
  const extraFetches = fetchCount() - before - 1; // minus the probe's own turn
  check('T4 probe turn started', probeStarted && speakStarted,
    `probe fetch=${probeStarted}, speak invoked=${speakStarted}`);
  check('T4b echo fingerprint armed before audio starts', probeStarted && extraFetches === 0,
    `echo fired between speak() and audio-start produced ${extraFetches} extra fetch(es)`);
  await settle();
}

// ============ T5: empty / whitespace final results never start turns ============
{
  const before = fetchCount();
  recInstance.__fire('', true);
  recInstance.__fire('   ', true);
  recInstance.__fire('x', true); // single char: text.length > 1 fails
  await sleep(120);
  check('T5 empty/whitespace/single-char results start no turns',
    fetchCount() - before === 0, `${fetchCount() - before} fetch(es)`);
}

// ============ T6: interim results never start turns ============
{
  const before = fetchCount();
  recInstance.__fire('interim thinking out loud', false);
  recInstance.__fire('still interim', false);
  await sleep(120);
  check('T6 interim results never start turns', fetchCount() - before === 0,
    `${fetchCount() - before} fetch(es)`);
}

const failed = results.filter(r => !r.pass);
console.log(`\n${results.length - failed.length}/${results.length} guard checks passed`);
const out = {
  harness: 'web/voice-guard-harness.mjs',
  target: 'web/voice.js (real source, stubbed browser env)',
  measured_at_utc: new Date().toISOString(),
  checks: results,
  verdict: failed.length === 0 ? 'ALL GUARDS VERIFIED' : 'GUARD FAILURE',
};
await import('fs').then(fs =>
  fs.writeFileSync(path.join(HERE, '..', 'fixtures', 'voice-path', 'guard-measurements.json'),
    JSON.stringify(out, null, 1)));
process.exit(failed.length ? 1 : 0);
