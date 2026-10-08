#!/usr/bin/env node
/* Prosody test harness. Run: node prosody-test.js
 * Loads web/prosody.js with a window shim, then asserts:
 *  1. wardrobe() yields exactly 60 combos, all axes strictly ordered
 *     rookie > prime > veteran for fixed (tone, position)
 *  2. forgiving parse: garbage era/tone/position -> prime/dry/middle
 *  3. emphasis is deterministic and tone-independent
 *  4. morphPlan is deterministic, walks era on schedule, labels positions
 *  5. header comment's 60-combo table matches wardrobe() output exactly
 */
'use strict';
const fs = require('fs');
const path = require('path');

const src = fs.readFileSync(path.join(__dirname, 'web', 'prosody.js'), 'utf8');
const window = {};
const fn = new Function('window', src + '\nreturn window.Prosody;');
const Prosody = fn(window);

let pass = 0, fail = 0;
function ok(cond, name, detail) {
  if (cond) { pass++; }
  else { fail++; console.log('FAIL:', name, detail === undefined ? '' : detail); }
}

// 1. wardrobe: 60 combos, strict era ordering on every axis
const w = Prosody.wardrobe();
ok(w.length === 60, 'wardrobe length 60', w.length);
const key = c => c.tone + '|' + c.position;
const byKey = {};
w.forEach(c => { (byKey[key(c)] = byKey[key(c)] || {})[c.era] = c; });
let ordered = true, orderedDetail = '';
Object.keys(byKey).forEach(k => {
  const g = byKey[k];
  if (!(g.rookie.rate > g.prime.rate && g.prime.rate > g.veteran.rate)) { ordered = false; orderedDetail = k + ' rate'; }
  if (!(g.rookie.pitch > g.prime.pitch && g.prime.pitch > g.veteran.pitch)) { ordered = false; orderedDetail = k + ' pitch'; }
  if (!(g.rookie.pause_ms < g.prime.pause_ms && g.prime.pause_ms < g.veteran.pause_ms)) { ordered = false; orderedDetail = k + ' pause'; }
});
ok(ordered, 'strict rookie>prime>veteran ordering on all axes', orderedDetail);

// clamps
let clamped = true;
w.forEach(c => {
  if (c.rate < 0.5 || c.rate > 2.0 || c.pitch < 0 || c.pitch > 2.0 || c.pause_ms < 0 || c.pause_ms > 3000) clamped = false;
});
ok(clamped, 'all attrs within clamp bounds');

// 2. forgiving parse
const f = Prosody.attrs('hacker', 'scream', 'nowhere');
ok(f.era === 'prime' && f.tone === 'dry' && f.position === 'middle', 'forgiving parse', JSON.stringify(f));
const d = Prosody.attrs();
ok(d.era === 'prime' && d.tone === 'dry', 'defaults with no args');

// determinism: same input, same output
ok(JSON.stringify(Prosody.attrs('veteran', 'steel', 'closing')) ===
   JSON.stringify(Prosody.attrs('veteran', 'steel', 'closing')), 'attrs deterministic');

// 3. emphasis: deterministic + tone-independent
const e1 = Prosody.attrsFor('The lake monster is unproven.', 'prime', 'dry', 'middle');
const e2 = Prosody.attrsFor('The lake monster is unproven.', 'rookie', 'steel', 'opening');
ok(e1.emphasis === e2.emphasis && e1.emphasis === 'unproven', 'emphasis tone-independent', e1.emphasis);
ok(Prosody.emphasisWord('a bb ccc') === 'ccc', 'longest-word rule');
ok(Prosody.emphasisWord('') === '', 'empty text -> empty emphasis');

// 4. morphPlan
const text = 'I started out chasing every ripple. Then I learned to read the water, and to wait. Now I only speak when the record does.';
const plan = Prosody.morphPlan(text, 'rookie', 'veteran');
ok(plan.length === 3, 'morph segments == sentences', plan.length);
ok(plan[0].era === 'rookie' && plan[1].era === 'prime' && plan[2].era === 'veteran',
   'era walks rookie->prime->veteran', plan.map(s => s.era).join(','));
ok(plan[0].attrs.position === 'opening' && plan[2].attrs.position === 'closing' && plan[1].attrs.position === 'middle',
   'position labels');
ok(JSON.stringify(Prosody.morphPlan(text, 'rookie', 'veteran')) === JSON.stringify(plan), 'morphPlan deterministic');
// no-morph case
const flat = Prosody.morphPlan('One sentence only.', 'prime', 'prime');
ok(flat.length === 1 && flat[0].era === 'prime' && flat[0].attrs.position === 'middle', 'no-morph single segment');
ok(Prosody.morphPlan('', 'rookie', 'veteran').length === 0, 'empty text -> no segments');
// reverse morph
const rev = Prosody.morphPlan('First. Second. Third.', 'veteran', 'rookie');
ok(rev[0].era === 'veteran' && rev[2].era === 'rookie', 'reverse morph', rev.map(s => s.era).join(','));
// long sentence clause-splits deterministically (>140 chars forces clause split)
const long = Prosody.morphPlan('She checked the receipts very carefully indeed, she checked every single timestamp twice, she verified the whole chain of custody line by line, and only then did she finally allow herself to believe it.', 'rookie', 'veteran');
ok(long.length >= 2, 'long sentence clause-split', long.length);
// attrs shape matches what voice.js speak() consumes
ok('rate' in plan[0].attrs && 'pitch' in plan[0].attrs && 'pause_ms' in plan[0].attrs, 'attrs shape for voice.js');

// 5. header table honesty: every wardrobe row appears verbatim in the comment
const fmt = c => `${c.era}/${c.tone}: ${(c.position + ' ').slice(0, 8)}${c.rate.toFixed(3)}/${c.pitch.toFixed(3)}/${c.pause_ms}`;
let tableOk = true, tableMiss = '';
w.forEach(c => {
  const needle = `${c.rate.toFixed(3)}/${c.pitch.toFixed(3)}/${c.pause_ms}`;
  if (!src.includes(needle)) { tableOk = false; tableMiss = fmt(c); }
});
ok(tableOk, 'header 60-combo table matches wardrobe()', tableMiss);

console.log(`\nprosody: ${pass} pass, ${fail} fail`);
process.exit(fail ? 1 : 0);
