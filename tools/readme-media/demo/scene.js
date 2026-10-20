// Ommatix protocol demo: a seekable, deterministic HyperFrames composition.
// window.DATA is injected by demo.mjs from docs/thesis/contratos/ejemplos.
// No clock, no random source, no animation library: every frame is a pure function of t.
(function () {
  const D = window.DATA;
  const W = 1280, H = 720, DURATION = 44;
  const C = {
    bg: '#0a1520', bg2: '#0e1c2b', surface: '#122236', surface2: '#18304a', line: '#2a4562',
    ink: '#eef4fb', muted: '#93a7bd', faint: '#5d7590', accent: '#5eead4', accentDim: '#134e4a',
    sky: '#7dd3fc', amber: '#fbbf24', amberDim: '#3b2a07', green: '#4ade80', red: '#f87171',
  };
  const SANS = "'Segoe UI Variable Display','Segoe UI',system-ui,sans-serif";
  const MONO = "'Cascadia Mono','Cascadia Code',Consolas,monospace";

  // ---------- math ----------
  const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
  const lerp = (a, b, p) => a + (b - a) * p;
  const prog = (t, a, b) => clamp((t - a) / (b - a));
  const inOut = (p) => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2);
  const out = (p) => 1 - Math.pow(1 - p, 3);
  const back = (p) => { const c1 = 1.70158, c3 = c1 + 1; return 1 + c3 * Math.pow(p - 1, 3) + c1 * Math.pow(p - 1, 2); };
  // fade in over [a,b], out over [c,d]
  const fade = (t, a, b, c, d) => (t < a || t > d ? 0 : t < b ? out(prog(t, a, b)) : t <= c ? 1 : 1 - inOut(prog(t, c, d)));
  const esc = (v) => String(v).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
  const f = (n) => Math.round(n * 100) / 100;

  // ---------- primitives ----------
  const rect = (x, y, w, h, o = {}) => `<rect x="${f(x)}" y="${f(y)}" width="${f(w)}" height="${f(h)}" rx="${o.r ?? 12}" fill="${o.fill ?? 'none'}" stroke="${o.stroke ?? 'none'}" stroke-width="${o.sw ?? 1}" opacity="${f(o.op ?? 1)}" ${o.extra ?? ''}/>`;
  const text = (x, y, s, o = {}) => `<text x="${f(x)}" y="${f(y)}" font-family="${o.mono ? MONO : SANS}" font-size="${o.size ?? 20}" font-weight="${o.weight ?? 400}" fill="${o.fill ?? C.ink}" opacity="${f(o.op ?? 1)}" text-anchor="${o.anchor ?? 'start'}" letter-spacing="${o.ls ?? 0}" ${o.extra ?? ''}>${esc(s)}</text>`;
  const g = (inner, o = {}) => `<g opacity="${f(o.op ?? 1)}" transform="${o.tr ?? ''}">${inner}</g>`;
  const hex = (cx, cy, r) => {
    const p = [];
    for (let i = 0; i < 6; i++) { const a = Math.PI / 3 * i + Math.PI / 6; p.push(`${f(cx + r * Math.cos(a))},${f(cy + r * Math.sin(a))}`); }
    return p.join(' ');
  };

  // ---------- layout ----------
  const WIN = { x: 48, y: 104, w: 680, h: 452 };
  const PANEL = { x: 808, y: 104, w: 424, h: 452 };
  const GRID = { x: 48, y: 148, cols: 17, rows: 10, s: 40 };
  const FIELD = { x: 80, y: 250, w: 400, h: 52 };
  const BTN = { x: 496, y: 476, w: 200, h: 52 };
  const GATE = { x: 768, y: 502 };
  const e7 = D.elements.find((e) => e.target_id === 'e7');
  const e8 = D.elements.find((e) => e.target_id === 'e8');

  // Scenes: [start, end, chip, caption]
  const SCENES = [
    [4.4, 10.2, '01 · Incremental perception', 'Only the regions that changed are analysed again.'],
    [10.2, 16.2, '02 · Stable identity', 'Each control gets a session ID and a revision. Coordinates stay inside the engine.'],
    [16.2, 23.0, '03 · Observe', 'The agent reads a compact list of elements, not pixels.'],
    [23.0, 30.0, '04 · Freshness', 'The observation is too old to authorise an action: refused, no input is sent.'],
    [30.0, 35.8, '05 · Observe again, then act', 'The new observation re-analyses only what changed. The dry run reports only what happened.'],
    [35.8, 39.6, '06 · Fail-closed actuation', 'Live input needs the operator’s policy and fresh evidence at every step.'],
  ];

  // ---------- background ----------
  function background(t) {
    let s = `<rect width="${W}" height="${H}" fill="${C.bg}"/>`;
    s += `<radialGradient id="glow" cx="0.5" cy="0.45" r="0.75"><stop offset="0" stop-color="#11324a" stop-opacity="0.55"/><stop offset="1" stop-color="${C.bg}" stop-opacity="0"/></radialGradient>`;
    s += `<rect width="${W}" height="${H}" fill="url(#glow)"/>`;
    // slow drifting dot field
    const drift = (t * 6) % 32;
    let dots = '';
    for (let y = -32; y < H + 32; y += 32) for (let x = -32; x < W + 32; x += 32) dots += `<circle cx="${f(x + drift)}" cy="${f(y + drift * 0.5)}" r="1" fill="${C.line}"/>`;
    s += g(dots, { op: 0.35 });
    return s;
  }

  // ---------- logo: a small ommatidia cluster ----------
  function logo(cx, cy, r, t, t0) {
    const cells = [[0, 0], [1, 0], [0.5, -0.866], [-0.5, -0.866], [-1, 0], [-0.5, 0.866], [0.5, 0.866]];
    let s = '';
    cells.forEach(([dx, dy], i) => {
      const p = out(prog(t, t0 + i * 0.09, t0 + i * 0.09 + 0.5));
      const lit = 0.5 + 0.5 * Math.sin((t - t0) * 2.2 + i * 0.9);
      s += `<polygon points="${hex(cx + dx * r * 1.78, cy + dy * r * 1.78, r * 0.92 * p)}" fill="${i === 0 ? C.accent : C.accentDim}" fill-opacity="${f(i === 0 ? 0.95 : 0.35 + 0.4 * lit)}" stroke="${C.accent}" stroke-opacity="${f(0.5 * p)}" stroke-width="1.5"/>`;
    });
    return s;
  }

  // ---------- intro / outro ----------
  function intro(t) {
    const op = fade(t, 0, 0.6, 3.6, 4.4);
    if (op <= 0) return '';
    const rise = lerp(18, 0, out(prog(t, 0.3, 1.2)));
    let s = logo(640, 250, 26, t, 0.1);
    s += text(640, 380 + rise, 'Ommatix', { size: 76, weight: 700, anchor: 'middle', ls: 1, op: out(prog(t, 0.4, 1.2)) });
    s += text(640, 428 + rise, 'Agents act on element IDs and revisions, not pixels.', { size: 26, fill: C.muted, anchor: 'middle', op: out(prog(t, 0.9, 1.7)) });
    s += text(640, 476 + rise, 'A protocol illustration on a simulated desktop', { size: 17, fill: C.faint, anchor: 'middle', op: out(prog(t, 1.4, 2.2)), ls: 0.5 });
    return g(s, { op });
  }

  function outro(t) {
    const op = fade(t, 39.6, 40.4, 43.4, 44.2);
    if (op <= 0) return '';
    let s = logo(640, 214, 24, t, 39.7);
    s += text(640, 336, 'Ommatix', { size: 72, weight: 700, anchor: 'middle', op: out(prog(t, 39.9, 40.7)) });
    s += text(640, 382, 'Spatial Indexing and Incremental Perception for GUI Agents', { size: 24, fill: C.muted, anchor: 'middle', op: out(prog(t, 40.2, 41.0)) });
    s += rect(640 - 190, 418, 380, 44, { fill: C.surface, stroke: C.line, r: 22, op: out(prog(t, 40.5, 41.3)) });
    s += text(640, 447, 'github.com/Yosoyepa/Ommatix', { size: 19, mono: true, fill: C.accent, anchor: 'middle', op: out(prog(t, 40.5, 41.3)) });
    s += text(640, 520, 'Research prototype · Rust · MCP · Windows 11 first', { size: 17, fill: C.muted, anchor: 'middle', op: out(prog(t, 40.9, 41.7)) });
    s += text(640, 548, 'Illustration on a simulated desktop, generated from the contract examples. Not a screen recording.', { size: 14, fill: C.faint, anchor: 'middle', op: out(prog(t, 41.1, 41.9)) });
    return g(s, { op });
  }

  // ---------- chrome: header, caption, progress ----------
  function chrome(t) {
    const op = fade(t, 4.2, 5.0, 39.2, 39.8);
    if (op <= 0) return '';
    let s = logo(62, 46, 7, t, 4.2);
    s += text(88, 52, 'OMMATIX', { size: 15, weight: 700, ls: 3 });
    s += `<path d="M196 38v18" stroke="${C.line}" stroke-width="1.5"/>`;
    s += text(210, 52, 'protocol illustration · simulated desktop · not a screen recording', { size: 14, fill: C.faint });
    for (const [a, b, chip, caption] of SCENES) {
      const o = fade(t, a, a + 0.5, b - 0.45, b);
      if (o <= 0) continue;
      const slide = lerp(12, 0, out(prog(t, a, a + 0.6)));
      const w = chip.length * 8.6 + 32;
      s += g(rect(W - 48 - w, 30, w, 30, { fill: C.accentDim, stroke: C.accent, r: 15, op: 0.9 }) + text(W - 48 - w / 2, 51, chip, { size: 14, weight: 600, fill: C.accent, anchor: 'middle' }), { op: o });
      s += g(text(48, 628 + slide, caption, { size: 27, weight: 600 }), { op: o });
    }
    // progress
    const p = prog(t, 4.4, 39.6);
    s += rect(48, 676, W - 96, 3, { fill: C.line, r: 2 });
    s += rect(48, 676, (W - 96) * p, 3, { fill: C.accent, r: 2 });
    SCENES.forEach(([a]) => { const x = 48 + (W - 96) * prog(a, 4.4, 39.6); s += `<circle cx="${f(x)}" cy="677.5" r="4" fill="${t >= a ? C.accent : C.line}"/>`; });
    return g(s, { op });
  }

  // ---------- the synthetic application window ----------
  function appWindow(t) {
    const op = fade(t, 4.6, 5.4, 35.6, 36.2);
    if (op <= 0) return '';
    const dy = lerp(30, 0, out(prog(t, 4.6, 5.6)));
    const { x, y, w, h } = WIN;
    let s = `<filter id="shadow" x="-10%" y="-10%" width="120%" height="130%"><feDropShadow dx="0" dy="14" stdDeviation="18" flood-color="#000" flood-opacity="0.45"/></filter>`;
    s += rect(x, y, w, h, { fill: C.bg2, stroke: C.line, r: 14, extra: 'filter="url(#shadow)"' });
    s += `<path d="M${x} ${y + 40}h${w}" stroke="${C.line}"/>`;
    ['#f87171', '#fbbf24', '#4ade80'].forEach((c, i) => { s += `<circle cx="${x + w - 70 + i * 22}" cy="${y + 20}" r="5.5" fill="${c}" opacity="0.55"/>`; });
    s += text(x + 20, y + 26, e8.label, { size: 15, fill: C.muted });
    // content
    s += text(80, 194, 'Documento', { size: 22, weight: 700 });
    s += rect(80, 210, 260, 9, { fill: C.surface2, r: 4 }) + rect(80, 226, 190, 9, { fill: C.surface2, r: 4 });
    // field with typing
    const typed = 'Synthetic document';
    const n = Math.floor(typed.length * prog(t, 6.0, 7.6));
    const editing = t >= 23.2;
    s += rect(FIELD.x, FIELD.y, FIELD.w, FIELD.h, { fill: C.surface, stroke: (t > 5.9 && t < 7.9) || (t > 23.1 && t < 24.0) ? C.sky : C.line, r: 10 });
    let value = typed.slice(0, n);
    if (editing) value = typed + ' v2'.slice(0, Math.floor(3 * prog(t, 23.2, 23.8)));
    s += text(FIELD.x + 18, FIELD.y + 33, value, { size: 18 });
    const caret = (t > 5.9 && t < 8.0) || (t > 23.1 && t < 24.2);
    if (caret && Math.floor(t * 2.5) % 2 === 0) {
      const cw = value.length * 8.9;
      s += `<rect x="${f(FIELD.x + 19 + cw)}" y="${FIELD.y + 15}" width="2" height="22" fill="${C.sky}"/>`;
    }
    // status line changes at t=23.4
    const status = t >= 23.4 ? 'Estado: editado' : t >= 7.6 ? 'Estado: borrador' : 'Estado: vacío';
    s += text(80, 342, status, { size: 16, fill: C.muted });
    s += rect(80, 368, 340, 9, { fill: C.surface2, r: 4 }) + rect(80, 384, 300, 9, { fill: C.surface2, r: 4 }) + rect(80, 400, 220, 9, { fill: C.surface2, r: 4 });
    // side card
    s += rect(520, 250, 176, 170, { fill: C.surface, stroke: C.line, r: 10 });
    s += rect(538, 270, 90, 9, { fill: C.surface2, r: 4 }) + rect(538, 292, 140, 40, { fill: C.surface2, r: 6 }) + rect(538, 344, 120, 9, { fill: C.surface2, r: 4 }) + rect(538, 362, 100, 9, { fill: C.surface2, r: 4 });
    // button e7
    s += rect(BTN.x, BTN.y, BTN.w, BTN.h, { fill: C.accentDim, stroke: C.accent, r: 10 });
    s += text(BTN.x + BTN.w / 2, BTN.y + 33, e7.label, { size: 18, weight: 600, fill: C.accent, anchor: 'middle' });
    // dry-run ghost press
    const ghost = prog(t, 33.0, 34.6);
    if (ghost > 0 && ghost < 1) {
      const rr = lerp(0, 120, out(ghost));
      s += `<circle cx="${BTN.x + BTN.w / 2}" cy="${BTN.y + BTN.h / 2}" r="${f(rr)}" fill="none" stroke="${C.accent}" stroke-width="2" stroke-dasharray="6 6" opacity="${f(1 - ghost)}"/>`;
    }
    return g(s, { op, tr: `translate(0 ${f(dy)})` });
  }

  // ---------- tile grid and changed regions ----------
  function tiles(t, from, to, cells, label) {
    const op = fade(t, from, from + 0.5, to - 0.6, to);
    if (op <= 0) return '';
    let s = '';
    for (let r = 0; r < GRID.rows; r++) for (let c = 0; c < GRID.cols; c++) {
      const x = GRID.x + c * GRID.s, y = GRID.y + r * GRID.s;
      const hit = cells(c, r);
      const appear = out(prog(t, from + (c + r) * 0.018, from + (c + r) * 0.018 + 0.4));
      if (hit) {
        const pulse = out(prog(t, from + 0.6 + c * 0.05, from + 1.0 + c * 0.05));
        s += `<rect x="${x + 1}" y="${y + 1}" width="${GRID.s - 2}" height="${GRID.s - 2}" rx="4" fill="${C.accent}" fill-opacity="${f(0.28 * pulse)}" stroke="${C.accent}" stroke-opacity="${f(0.95 * pulse)}" stroke-width="1.5"/>`;
      } else {
        s += `<rect x="${x + 1}" y="${y + 1}" width="${GRID.s - 2}" height="${GRID.s - 2}" rx="4" fill="${C.bg}" fill-opacity="${f(0.42 * appear)}" stroke="${C.line}" stroke-opacity="${f(0.7 * appear)}"/>`;
      }
    }
    if (!label) return g(s, { op });
    // legend
    const lo = out(prog(t, from + 1.0, from + 1.6));
    s += g(rect(WIN.x + WIN.w + 16, 300, 196, 92, { fill: C.surface, stroke: C.line, r: 12 }) +
      `<rect x="${WIN.x + WIN.w + 32}" y="318" width="16" height="16" rx="3" fill="${C.accent}" fill-opacity="0.3" stroke="${C.accent}"/>` +
      text(WIN.x + WIN.w + 58, 331, label, { size: 15, fill: C.ink }) +
      `<rect x="${WIN.x + WIN.w + 32}" y="352" width="16" height="16" rx="3" fill="${C.bg}" stroke="${C.line}"/>` +
      text(WIN.x + WIN.w + 58, 365, 'unchanged · reused', { size: 15, fill: C.muted }), { op: lo });
    return g(s, { op });
  }

  // ---------- identity boxes ----------
  function identity(t) {
    const op = fade(t, 10.6, 11.0, 22.6, 23.0);
    if (op <= 0) return '';
    let s = '';
    // tag placed above the box, or to its right (side = true)
    const box = (x, y, w, h, t0, tag, sub, side) => {
      const p = inOut(prog(t, t0, t0 + 0.8));
      const per = 2 * (w + h);
      let b = `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="8" fill="none" stroke="${C.sky}" stroke-width="2" stroke-dasharray="${f(per)}" stroke-dashoffset="${f(per * (1 - p))}"/>`;
      const tp = back(prog(t, t0 + 0.6, t0 + 1.1));
      const tw = Math.max(tag.length * 9.4, sub.length * 7.6) + 26;
      const tx = side ? x + w + 12 : x, ty = side ? y + h / 2 - 23 : y - 54;
      b += g(rect(tx, ty, tw, 46, { fill: C.surface2, stroke: C.sky, r: 8 }) +
        text(tx + 13, ty + 20, tag, { size: 16, weight: 700, mono: true, fill: C.sky }) +
        text(tx + 13, ty + 39, sub, { size: 13, mono: true, fill: C.muted }), { op: clamp(tp), tr: `translate(${f(tx)} ${f(ty)}) scale(${f(lerp(0.85, 1, clamp(tp)))}) translate(${f(-tx)} ${f(-ty)})` });
      return b;
    };
    s += box(60, 110, 196, 30, 11.0, `${e8.target_id} · rev ${e8.revision}`, `${e8.role} · ${e8.actionability}`, true);
    s += box(BTN.x - 6, BTN.y - 6, BTN.w + 12, BTN.h + 12, 11.8, `${e7.target_id} · rev ${e7.revision}`, `${e7.role} · ${e7.actionability}`);
    // what crosses the boundary
    const np = fade(t, 13.0, 13.6, 15.6, 16.1);
    const bx = WIN.x + WIN.w + 32;
    s += g(rect(bx, 236, 420, 196, { fill: C.surface, stroke: C.line, r: 14 }) +
      text(bx + 24, 274, 'Sent to the agent', { size: 15, fill: C.accent, weight: 700, ls: 0.5 }) +
      text(bx + 24, 306, 'target_id · revision · role · label', { size: 17, mono: true, fill: C.ink }) +
      text(bx + 24, 330, 'actionability', { size: 17, mono: true, fill: C.ink }) +
      `<path d="M${bx + 24} 352h372" stroke="${C.line}"/>` +
      text(bx + 24, 382, 'Kept inside the engine', { size: 15, fill: C.muted, weight: 700, ls: 0.5 }) +
      text(bx + 24, 410, 'pixels · coordinates · bounding boxes', { size: 17, mono: true, fill: C.muted }), { op: np, tr: `translate(${f(lerp(16, 0, np))} 0)` });
    return g(s, { op });
  }

  // ---------- agent panel and MCP log ----------
  const LOG = [
    { t: 16.9, k: 'call', l: ['observe_screen {}'] },
    { t: 17.6, k: 'resp', l: [`snapshot_id: "s16"`, `${e7.target_id}  ${e7.role}  "${e7.label}"`, `    revision ${e7.revision} · ${e7.actionability}`, `${e8.target_id}  ${e8.role}  "${e8.label}"`, `    revision ${e8.revision} · ${e8.actionability}`] },
    { t: 24.4, k: 'call', l: ['execute_action', `  target_id "${e7.target_id}" · expected_revision ${e7.revision}`, `  snapshot_id "${D.stale.snapshot_id}" · dry_run true`] },
    { t: 26.0, k: 'err', l: [`${D.stale.status} · ${D.stale.error.code}`, `  input_delivered: ${D.stale.input_delivered}`, `  next_step: ${D.stale.error.next_step}`] },
    { t: 30.3, k: 'call', l: ['observe_screen {}'] },
    { t: 30.9, k: 'resp', l: [`snapshot_id: "${D.click.snapshot_id}" · ${e7.target_id} revision ${e7.revision}`] },
    { t: 31.6, k: 'call', l: ['execute_action', `  target_id "${D.click.target_id}" · expected_revision ${D.click.expected_revision}`, `  snapshot_id "${D.click.snapshot_id}" · dry_run ${D.click.dry_run}`] },
    { t: 33.4, k: 'ok', l: [`${D.sim.status}`, `  input_delivered: ${D.sim.input_delivered}`, `  verified: ${D.sim.verified}`] },
  ];
  const KIND = { call: [C.sky, '→'], resp: [C.ink, '←'], err: [C.amber, '←'], ok: [C.green, '←'] };

  function panel(t) {
    const op = fade(t, 16.2, 16.8, 35.6, 36.2);
    if (op <= 0) return '';
    const dx = lerp(40, 0, out(prog(t, 16.2, 17.0)));
    const { x, y, w, h } = PANEL;
    let s = rect(x, y, w, h, { fill: C.bg2, stroke: C.line, r: 14, extra: 'filter="url(#shadow)"' });
    s += `<path d="M${x} ${y + 40}h${w}" stroke="${C.line}"/>`;
    s += `<circle cx="${x + 22}" cy="${y + 20}" r="5" fill="${C.accent}"/>`;
    s += text(x + 36, y + 26, 'LLM agent · MCP client', { size: 15, fill: C.muted });
    // lines
    const LH = 21, top = y + 70;
    let rows = [];
    for (const e of LOG) e.l.forEach((line, i) => rows.push({ at: e.t + i * 0.12, k: e.k, first: i === 0, line, gapBefore: i === 0 }));
    let yy = 0;
    const placed = rows.map((r) => { if (r.gapBefore) yy += 8; const o = { ...r, y: yy }; yy += LH; return o; });
    const visible = placed.filter((r) => t >= r.at);
    const last = visible.length ? visible[visible.length - 1].y : 0;
    const maxY = h - 96;
    const scrollTarget = Math.max(0, last - maxY);
    // smooth scroll: interpolate towards target based on the latest row's age
    let scroll = 0;
    if (visible.length) {
      const prev = visible.length > 1 ? Math.max(0, visible[visible.length - 2].y - maxY) : 0;
      scroll = lerp(prev, scrollTarget, out(prog(t, visible[visible.length - 1].at, visible[visible.length - 1].at + 0.35)));
    }
    let body = '';
    for (const r of visible) {
      const a = out(prog(t, r.at, r.at + 0.35));
      const ry = top + r.y - scroll;
      if (ry < y + 52 || ry > y + h - 12) continue;
      const [col, arrow] = KIND[r.k];
      const isCall = r.k === 'call';
      if (r.first) body += text(x + 20, ry, arrow, { size: 16, mono: true, fill: col, op: a });
      body += text(x + 42 + lerp(10, 0, a), ry, r.line, { size: 15, mono: true, fill: r.first ? col : C.muted, weight: r.first && isCall ? 700 : 400, op: a });
    }
    s += `<clipPath id="logclip"><rect x="${x}" y="${y + 48}" width="${w}" height="${h - 56}"/></clipPath><g clip-path="url(#logclip)">${body}</g>`;
    return g(s, { op, tr: `translate(${f(dx)} 0)` });
  }

  // ---------- request packet travelling to the gate ----------
  function packet(t, t0, refused) {
    const p = prog(t, t0, t0 + 1.1);
    if (t < t0 || t > t0 + 2.6) return '';
    const sx = PANEL.x - 6, sy = PANEL.y + 230, gx = GATE.x, gy = GATE.y, bx = BTN.x + BTN.w + 8, by = BTN.y + BTN.h / 2;
    let s = '';
    // gate shield
    const gp = fade(t, t0 - 0.2, t0 + 0.2, t0 + 2.2, t0 + 2.6);
    const shield = `M${gx} ${gy - 34} l26 10 v18 c0 18 -13 29 -26 36 c-13 -7 -26 -18 -26 -36 v-18 z`;
    const gateCol = refused && t > t0 + 1.1 ? C.amber : t > t0 + 1.1 ? C.green : C.accent;
    s += g(`<path d="${shield}" fill="${C.surface2}" stroke="${gateCol}" stroke-width="2.5"/>` + text(gx, gy + 5, 'gate', { size: 12, anchor: 'middle', fill: gateCol, weight: 700 }), { op: gp });
    // path
    s += `<path d="M${sx} ${sy} C ${sx - 30} ${sy + 120}, ${gx + 40} ${gy - 60}, ${gx} ${gy - 30}" fill="none" stroke="${C.line}" stroke-width="1.5" stroke-dasharray="4 5" opacity="${f(gp)}"/>`;
    if (!refused) s += `<path d="M${gx - 22} ${gy} L ${bx} ${by}" fill="none" stroke="${C.line}" stroke-width="1.5" stroke-dasharray="4 5" opacity="${f(gp)}"/>`;
    // dot
    const bez = (u) => {
      const P0 = [sx, sy], P1 = [sx - 30, sy + 120], P2 = [gx + 40, gy - 60], P3 = [gx, gy - 30];
      const m = 1 - u;
      return [0, 1].map((k) => m * m * m * P0[k] + 3 * m * m * u * P1[k] + 3 * m * u * u * P2[k] + u * u * u * P3[k]);
    };
    if (p < 1) {
      const [px, py] = bez(inOut(p));
      s += `<circle cx="${f(px)}" cy="${f(py)}" r="6" fill="${C.sky}"/><circle cx="${f(px)}" cy="${f(py)}" r="12" fill="${C.sky}" opacity="0.25"/>`;
    } else if (refused) {
      const q = prog(t, t0 + 1.1, t0 + 1.8);
      const [px, py] = bez(1 - 0.35 * out(q));
      s += `<circle cx="${f(px)}" cy="${f(py)}" r="${f(6 * (1 - q))}" fill="${C.amber}" opacity="${f(1 - q)}"/>`;
    } else {
      const q = inOut(prog(t, t0 + 1.1, t0 + 1.9));
      if (q < 1) {
        const px = lerp(gx - 22, bx, q), py = lerp(gy, by, q);
        s += `<circle cx="${f(px)}" cy="${f(py)}" r="6" fill="${C.accent}"/><circle cx="${f(px)}" cy="${f(py)}" r="12" fill="${C.accent}" opacity="0.25"/>`;
      }
    }
    return s;
  }

  // ---------- snapshot age, STALE stamp ----------
  // ADR-0025: a snapshot older than max_snapshot_age_ms (10 000 ms by default) does not authorise
  // an action, in a dry run too. The engine publishes a new snapshot only when the agent observes.
  const OBSERVED = 17.6, REOBSERVED = 30.9, AGE_RATE = 1.6; // the illustration compresses the clock
  function stale(t) {
    let s = '';
    const chipX = WIN.x + WIN.w - 290, chipY = WIN.y + 52;
    const chip = (label, value, col, note) =>
      rect(chipX, chipY, 272, 34, { fill: C.surface2, stroke: col === C.muted ? C.line : col, r: 17 }) +
      text(chipX + 16, chipY + 22, label, { size: 13, mono: true, fill: C.muted }) +
      text(chipX + 256, chipY + 22, value, { size: 14, mono: true, fill: col, weight: 700, anchor: 'end' }) +
      (note ? text(chipX + 136, chipY + 52, note, { size: 12, fill: col, anchor: 'middle' }) : '');
    const ao = fade(t, OBSERVED, OBSERVED + 0.4, REOBSERVED - 0.5, REOBSERVED - 0.1);
    if (ao > 0) {
      const age = Math.max(0, t - OBSERVED) * AGE_RATE;
      const over = age > 10;
      s += g(chip('snapshot s16 · age', `${age.toFixed(1)} s`, over ? C.amber : C.muted,
        over ? 'older than the 10 s limit · clock compressed' : 'clock compressed'), { op: ao });
    }
    const no = fade(t, REOBSERVED, REOBSERVED + 0.4, 35.4, 35.8);
    if (no > 0) {
      const age = Math.max(0, t - REOBSERVED) * AGE_RATE;
      s += g(chip('snapshot s17 · age', `${age.toFixed(1)} s`, C.accent, 'published by the new observation'), { op: no });
    }
    // stamp
    const so = fade(t, 25.5, 25.7, 29.2, 29.8);
    if (so > 0) {
      const sc = lerp(1.6, 1, back(prog(t, 25.5, 25.95)));
      const cx = WIN.x + WIN.w / 2, cy = WIN.y + 250;
      const shake = t < 26.2 ? Math.sin(t * 90) * 3 * (1 - prog(t, 25.6, 26.2)) : 0;
      const stamp = rect(cx - 210, cy - 54, 420, 108, { fill: C.amberDim, stroke: C.amber, sw: 3, r: 14, op: 0.96 }) +
        text(cx, cy - 4, D.stale.error.code, { size: 40, weight: 800, fill: C.amber, anchor: 'middle', mono: true, ls: 1 }) +
        text(cx, cy + 32, 's16 is too old · no input sent · observe again', { size: 18, fill: C.amber, anchor: 'middle' });
      s += g(stamp, { op: so, tr: `translate(${f(cx + shake)} ${f(cy)}) rotate(-5) scale(${f(sc)}) translate(${f(-cx)} ${f(-cy)})` });
    }
    return s;
  }

  // ---------- the safety gate chain ----------
  function chain(t) {
    const op = fade(t, 36.0, 36.5, 39.2, 39.7);
    if (op <= 0) return '';
    let s = '';
    s += text(640, 196, 'Every action is a dry run unless the agent asks otherwise and the operator’s policy allows it.', { size: 19, fill: C.muted, anchor: 'middle', op: out(prog(t, 36.1, 36.6)) });
    const steps = ['Operator policy', 'Foreground app', 'Fresh snapshot', 'Last look', 'Deliver input', 'Verify effect'];
    const cw = 168, gap = 22, total = steps.length * cw + (steps.length - 1) * gap, x0 = (W - total) / 2, y = 268;
    steps.forEach((label, i) => {
      const a = 36.5 + i * 0.4;
      const p = out(prog(t, a, a + 0.4));
      const lit = t >= a + 0.2;
      const x = x0 + i * (cw + gap);
      s += g(rect(x, y + lerp(16, 0, p), cw, 64, { fill: lit ? C.accentDim : C.surface, stroke: lit ? C.accent : C.line, sw: 1.5, r: 12 }) +
        text(x + 16, y + 26 + lerp(16, 0, p), String(i + 1).padStart(2, '0'), { size: 12, mono: true, fill: lit ? C.accent : C.faint }) +
        text(x + 16, y + 48 + lerp(16, 0, p), label, { size: 17, weight: 600, fill: lit ? C.ink : C.muted }), { op: p });
      if (i < steps.length - 1) s += g(`<path d="M${x + cw + 4} ${y + 32}h${gap - 8}m-6 -5 6 5 -6 5" fill="none" stroke="${C.accent}" stroke-width="2"/>`, { op: out(prog(t, a + 0.3, a + 0.6)) });
    });
    const no = out(prog(t, 38.4, 38.9));
    s += g(rect(640 - 330, 380, 660, 64, { fill: C.surface, stroke: C.line, r: 12 }) +
      text(640, 408, 'A check before delivery fails → refused, nothing delivered.', { size: 18, anchor: 'middle', weight: 600 }) +
      text(640, 432, 'After delivery, an uncertain effect is reported as indeterminate, never retried.', { size: 15, anchor: 'middle', fill: C.muted }), { op: no });
    return g(s, { op });
  }

  function frame(t) {
    let s = background(t);
    s += intro(t);
    s += appWindow(t);
    s += tiles(t, 6.6, 10.4, (c, r) => c <= 10 && r >= 2 && r <= 3, 'changed · re-analysed');
    s += tiles(t, 30.3, 32.2, (c, r) => c <= 10 && r >= 2 && r <= 4, null);
    s += identity(t);
    s += panel(t);
    s += packet(t, 24.5, true);
    s += packet(t, 31.8, false);
    s += stale(t);
    s += chain(t);
    s += outro(t);
    s += chrome(t);
    return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">${s}</svg>`;
  }

  function draw(t) { document.getElementById('root').innerHTML = frame(Math.max(0, Math.min(DURATION, t))); }
  window.draw = draw;
  // HyperFrames' seekable timeline interface.
  let current = 0;
  const timeline = {
    duration: () => DURATION, totalDuration: () => DURATION, getChildren: () => [], pause() { return this; }, paused: () => true,
    seek(t) { current = t; draw(t); return this; },
    time(t) { if (t === undefined) return current; return this.seek(t); },
    totalTime(t) { if (t === undefined) return current; return this.seek(t); },
  };
  window.__timelines = window.__timelines || {};
  window.__timelines.protocol = timeline;
  draw(0);
})();
