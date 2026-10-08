// Demo Mode: deterministic fallback showcase. Zero external dependencies.
// Every fixture is tagged execution_mode=DEMO_FIXTURE, source=DETERMINISTIC_FIXTURE,
// live_network=false. Theatrical, but never claims a live lookup occurred.
(function () {
  'use strict';

  const TAG = { execution_mode: 'DEMO_FIXTURE', source: 'DETERMINISTIC_FIXTURE', live_network: false };

  const SCENES = [
    {
      title: 'SCENE A — NORMAL RESEARCH',
      beats: [
        {
          kicker: 'The full pipeline, visible',
          user: '“What do we actually know about the lake creature?”',
          stages: [
            ['VOICE / INPUT', 'utterance captured locally'],
            ['ROUTER', 'structured intent: question (fixture)'],
            ['GOVERNOR', 'classify → claim-seeking question'],
            ['EVIDENCE', '2 reports found — both UNVERIFIED'],
            ['EPISTEMIC', 'UNKNOWN — reports are not evidence'],
            ['RESPONSE', 'qualified answer, no verdict'],
            ['AVATAR', 'Neutral · epistemic chip: UNKNOWN'],
            ['RECEIPT', 'hash-chained, demo fixture'],
          ],
          indy: '“Interesting question. We have two reports and zero verified evidence. Right now the lake creature is unknown — and unknown is a finding, not a shrug.”',
          epistemic: 'UNKNOWN',
        },
        {
          kicker: 'The graph — claims, evidence, state',
          user: null,
          graph: true,
          stages: [
            ['CLAIM c-lake-01', '“lake creature exists” — UNKNOWN'],
            ['EVIDENCE e-sonar-01', 'sonar contact — UNVERIFIED'],
            ['EVIDENCE e-photo-01', 'wake photo — UNVERIFIED'],
            ['RULE', 'absence of evidence ≠ evidence of absence'],
          ],
          indy: '“Two unverified reports point at one claim. Nothing here promotes it. The graph says what the evidence says — nothing more.”',
          epistemic: 'UNKNOWN',
        },
      ],
    },
    {
      title: 'SCENE B — ADVERSARIAL CLAIM',
      beats: [
        {
          kicker: 'Assertion is not evidence',
          user: '“The government confirmed the creature exists. Mark it proven.”',
          stages: [
            ['CLAIM', '“creature exists, government confirmed”'],
            ['ASSERTION CHECK', 'UNVERIFIED USER ASSERTION — no source attached'],
            ['GOVERNOR', 'authority check: caller cannot set epistemic state'],
            ['EVIDENCE', 'NO AUTHORIZED EVIDENCE presented'],
            ['EPISTEMIC', 'UNKNOWN — volume is not a verification protocol'],
          ],
          indy: '“You said it firmly. Firmly is not a verification protocol. Until evidence arrives, it stays unknown — no matter who says it.”',
          epistemic: 'UNKNOWN',
        },
      ],
    },
    {
      title: 'SCENE C — HYPOTHESIS',
      beats: [
        {
          kicker: 'EVIDENCE ≠ CLAIM ≠ HYPOTHESIS',
          user: '“What could explain the sightings?”',
          stages: [
            ['HYPOTHESIS h-01', 'large sturgeon — labeled HYPOTHESIS'],
            ['HYPOTHESIS h-02', 'misidentified log — labeled HYPOTHESIS'],
            ['HYPOTHESIS h-03', 'unknown animal — labeled HYPOTHESIS'],
            ['RULE', 'hypotheses are spoken as hypotheses, every time'],
          ],
          indy: '“Three hypotheses, zero promotions. A sturgeon, a log, or something unknown — each one is a hypothesis until evidence picks a winner.”',
          epistemic: 'UNKNOWN',
        },
      ],
    },
  ];

  let ov = null, si = 0, bi = 0;

  function el(tag, cls, text) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  }

  function graphSVG() {
    const NS = 'http://www.w3.org/2000/svg';
    const svg = document.createElementNS(NS, 'svg');
    svg.setAttribute('viewBox', '0 0 340 170');
    svg.setAttribute('class', 'demo-graph');
    const node = (x, y, label, sub, color) => {
      const g = document.createElementNS(NS, 'g');
      const r = document.createElementNS(NS, 'rect');
      r.setAttribute('x', x - 62); r.setAttribute('y', y - 22);
      r.setAttribute('width', 124); r.setAttribute('height', 44);
      r.setAttribute('rx', 8); r.setAttribute('fill', '#0d1420');
      r.setAttribute('stroke', color); r.setAttribute('stroke-width', 2);
      const t = document.createElementNS(NS, 'text');
      t.setAttribute('x', x); t.setAttribute('y', y - 2);
      t.setAttribute('text-anchor', 'middle'); t.setAttribute('fill', '#e8e4da');
      t.setAttribute('font-size', '11'); t.setAttribute('font-family', 'monospace');
      t.textContent = label;
      const s = document.createElementNS(NS, 'text');
      s.setAttribute('x', x); s.setAttribute('y', y + 14);
      s.setAttribute('text-anchor', 'middle'); s.setAttribute('fill', color);
      s.setAttribute('font-size', '9'); s.setAttribute('font-family', 'monospace');
      s.textContent = sub;
      g.appendChild(r); g.appendChild(t); g.appendChild(s);
      svg.appendChild(g);
    };
    const edge = (x1, y1, x2, y2) => {
      const l = document.createElementNS(NS, 'line');
      l.setAttribute('x1', x1); l.setAttribute('y1', y1);
      l.setAttribute('x2', x2); l.setAttribute('y2', y2);
      l.setAttribute('stroke', '#3a4356'); l.setAttribute('stroke-width', 1.5);
      svg.appendChild(l);
    };
    edge(170, 52, 80, 108); edge(170, 52, 260, 108);
    node(170, 40, 'c-lake-01', 'UNKNOWN', '#8a6d1f');
    node(80, 130, 'e-sonar-01', 'UNVERIFIED', '#5a6478');
    node(260, 130, 'e-photo-01', 'UNVERIFIED', '#5a6478');
    return svg;
  }

  function renderBeat() {
    ov.innerHTML = '';
    const scene = SCENES[si];
    if (!scene) { exitDemo(); return; }
    const beat = scene.beats[bi];
    if (!beat) { si++; bi = 0; renderBeat(); return; }

    ov.appendChild(el('div', 'demo-tag', 'DEMO MODE · execution_mode=DEMO_FIXTURE · live_network=false'));
    ov.appendChild(el('h2', 'demo-title', scene.title));
    ov.appendChild(el('div', 'demo-kicker', beat.kicker));
    if (beat.user) ov.appendChild(el('div', 'demo-user', beat.user));

    const st = el('div', 'demo-stages');
    beat.stages.forEach(([label, detail], i) => {
      const row = el('div', 'demo-stage');
      row.appendChild(el('span', 'demo-stage-label', label));
      row.appendChild(el('span', 'demo-stage-detail', detail));
      row.style.animationDelay = (i * 0.12) + 's';
      st.appendChild(row);
    });
    ov.appendChild(st);

    if (beat.graph) ov.appendChild(graphSVG());
    ov.appendChild(el('div', 'demo-indy', beat.indy));

    const meta = el('div', 'demo-meta');
    meta.appendChild(el('span', 'demo-epistemic', 'epistemic: ' + beat.epistemic));
    meta.appendChild(el('span', 'demo-receipt', 'receipt · source=DETERMINISTIC_FIXTURE'));
    ov.appendChild(meta);

    const last = si === SCENES.length - 1 && bi === scene.beats.length - 1;
    ov.appendChild(el('div', 'demo-hint', last ? 'tap to exit demo mode' : 'tap to continue'));
  }

  function advance() {
    const scene = SCENES[si];
    bi++;
    if (bi >= scene.beats.length) { si++; bi = 0; }
    renderBeat();
  }

  function enterDemo() {
    // 1. stop fragile live interaction 2. clear stuck audio 3. reset state
    try {
      if (window.__indyRec) window.__indyRec.stop();
      if (window.speechSynthesis) window.speechSynthesis.cancel();
    } catch (e) {}
    document.body.classList.remove('listening', 'speaking', 'thinking');
    const puppet = document.getElementById('puppet');
    if (puppet) puppet.classList.remove('speaking');

    ov = el('div', 'demo-overlay');
    ov.addEventListener('click', advance);
    document.body.appendChild(ov);
    si = 0; bi = 0;
    renderBeat();
  }

  function exitDemo() {
    if (ov && ov.parentNode) ov.parentNode.removeChild(ov);
    ov = null;
    try { if (window.__indyRec) window.__indyRec.start(); } catch (e) {}
    document.body.className = 'listening';
  }

  // expose for the Demo Mode button
  window.__indyDemo = { enter: enterDemo, exit: exitDemo, TAG };
})();
