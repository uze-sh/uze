'use client';

import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { UzeMark } from '@/components/uze-mark';

// The landing page's hero illustration: one plugin resolved from the Store,
// delivered to four harnesses, then read through the three extensions in the
// order a change is read. It is drawn on a fixed stage and scaled to the
// column: 1200×500 left to right where there is room, and the same three
// panels stacked top to bottom on a phone, where the wide stage would scale
// its text below reading size. The palette is the site's own theme tokens,
// so it follows the page into light or dark.

// Every colour is one of the site's theme tokens, or a mix of them, so the
// illustration reads as part of the page in either theme rather than as a
// dark screenshot pasted onto a light one.
const mix = (color: string, percent: number, base = 'transparent') =>
  `color-mix(in srgb, ${color} ${percent}%, ${base})`;
const INK = 'var(--color-ink)';
const PAPER = 'var(--color-paper)';
const MUTED = 'var(--color-muted)';
const G = 'var(--color-accent)';
const AMB = '#d4a72c';
const DIM = 'var(--color-line)';
const LINE = 'var(--color-line)';
const HOT = mix(G, 70);
const SOFT = mix(G, 40, DIM);
const FAINT = mix(MUTED, 55);
// Flat, not outlined: depth is opaque fills stepping from the page toward
// the ink, neutral in both themes — the surface token is tinted green in the
// dark one, and a whole panel of it reads as a cast. Green is kept for the
// marks: tags, checks, routes and the lit extension.
const PANEL = mix(INK, 4, PAPER);
const CARD = mix(INK, 7, PAPER);
const CARD_ON = mix(INK, 12, PAPER);
const SHEET = mix(INK, 9, PAPER);
const REMOVED = '#e5534b';

const ROWS: [string, string][] = [
  ['AGENTS.md', 'md'],
  ['skills/', 'S'],
  ['mcp/', 'M'],
  ['agents/', 'A'],
  ['hooks/', 'H'],
];
const HARNESSES = ['Claude Code', 'Codex', 'OpenCode', 'Antigravity'];
const SLOTS = ['md', 'S', 'M', 'A', 'H'];
// Antigravity receives its hooks as a generated plugin: the one route here
// that is not the harness's own.
const ROUTE = [
  ['n', 'n', 'n', 'n', 'n'],
  ['n', 'n', 'n', 'n', 'n'],
  ['n', 'n', 'n', 'n', 'a'],
  ['n', 'n', 'n', 'n', 'n'],
];
const EXTENSIONS = ['Spec', 'Architect', 'Code'];
const EXT_TITLE = ['spec · add-review', 'architect · docs/architecture', 'code · src/review/hook.rs'];
const EXT_META = ['3 of 7 tasks', 'C4 · 3 containers', '+42 −7'];

const SPEC_TASKS: [string, boolean][] = [
  ['Define review rules', true],
  ['Read agents.yaml config', true],
  ['Wire pre-commit hook', true],
  ['Write scenario tests', false],
  ['Document the skill', false],
  ['Archive the change', false],
];
const ADDED = mix(G, 12);
const DIFF: [number, string, string, string, string][] = [
  [12, ' ', 'fn on_commit(ctx: &Ctx) -> Result {', '', MUTED],
  [13, '-', '    let rules = default_rules();', mix(REMOVED, 12), REMOVED],
  [14, '+', '    let rules = ctx.config.review_rules()?;', ADDED, G],
  [15, '+', '    let diff = ctx.git.staged_diff()?;', ADDED, G],
  [16, ' ', '    for rule in rules {', '', MUTED],
  [17, '+', '        rule.check(&diff)?;', ADDED, G],
  [18, ' ', '    }', '', MUTED],
];

// The script's clock, in seconds of one loop.
const T_TYPE = 1.0;
const T_ROW = 1.2;
const ROW_GAP = 0.26;
const T_HEX = 2.7;
const T_DLV = 2.85;
const T_DLV_END = 3.5;
const T_RUN = 3.9;
const T_EXT = 5.0;
const EXT_GAP = 1.7;
const T_DONE = 10.3;
const LOOP = 11.9;
// What a reader who asked for no motion sees: delivered everywhere, the
// spec surface open.
const STILL_AT = 5.8;

type Point = [number, number];

const clamp = (x: number) => Math.max(0, Math.min(1, x));
const ease = (x: number) => (x < 0.5 ? 2 * x * x : 1 - Math.pow(-2 * x + 2, 2) / 2);

function roundedPath(points: Point[], radius = 14) {
  let d = `M${points[0][0]} ${points[0][1]}`;
  for (let i = 1; i < points.length - 1; i++) {
    const [px, py] = points[i - 1];
    const [cx, cy] = points[i];
    const [nx, ny] = points[i + 1];
    const before = Math.hypot(cx - px, cy - py);
    const after = Math.hypot(nx - cx, ny - cy);
    if (!before || !after) {
      d += ` L${cx} ${cy}`;
      continue;
    }
    const r = Math.min(radius, before / 2, after / 2);
    const ax = cx - ((cx - px) / before) * r;
    const ay = cy - ((cy - py) / before) * r;
    const bx = cx + ((nx - cx) / after) * r;
    const by = cy + ((ny - cy) / after) * r;
    d += ` L${ax} ${ay} Q${cx} ${cy} ${bx} ${by}`;
  }
  const [lx, ly] = points[points.length - 1];
  return `${d} L${lx} ${ly}`;
}

function pointAlong(points: Point[], progress: number): Point {
  const lengths: number[] = [];
  let total = 0;
  for (let i = 1; i < points.length; i++) {
    const length = Math.hypot(points[i][0] - points[i - 1][0], points[i][1] - points[i - 1][1]);
    lengths.push(length);
    total += length;
  }
  let remaining = progress * total;
  for (let i = 1; i < points.length; i++) {
    if (remaining <= lengths[i - 1]) {
      const f = remaining / lengths[i - 1];
      return [
        points[i - 1][0] + (points[i][0] - points[i - 1][0]) * f,
        points[i - 1][1] + (points[i][1] - points[i - 1][1]) * f,
      ];
    }
    remaining -= lengths[i - 1];
  }
  return points[points.length - 1];
}

type Layout = {
  width: number;
  height: number;
  store: Point;
  hub: Point;
  status: Point;
  workspace: Point;
  rowRoute: (r: number) => Point[];
  harnessRoute: (j: number) => Point[];
};

const WIDE: Layout = {
  width: 1200,
  height: 500,
  store: [60, 50],
  hub: [536, 184],
  status: [480, 330],
  workspace: [820, 50],
  rowRoute: (r) => [
    [360, 140 + 54 * r],
    [440, 140 + 54 * r],
    [440, 248],
    [536, 248],
  ],
  harnessRoute: (j) => [
    [664, 248],
    [760, 248],
    [760, 130 + 68 * j],
    [820, 130 + 68 * j],
  ],
};

// The routes to the harnesses leave the hub by its side and run down the
// left gutter, so they never cross the status line under it.
const STACKED: Layout = {
  width: 380,
  height: 1110,
  store: [40, 20],
  hub: [126, 480],
  status: [70, 624],
  workspace: [30, 690],
  rowRoute: (r) => [
    [340, 110 + 54 * r],
    [362, 110 + 54 * r],
    [362, 452],
    [190, 452],
    [190, 480],
  ],
  harnessRoute: (j) => [
    [126, 544],
    [10, 544],
    [10, 770 + 68 * j],
    [30, 770 + 68 * j],
  ],
};

// Below this width the wide stage's 15px text would draw at under 9px.
const STACK_BELOW = 700;

function frame(seconds: number, plugin: string, { rowRoute, harnessRoute }: Layout) {
  const tt = seconds % LOOP;
  const reset = tt > LOOP - 0.3;
  const dots: Point[] = [];

  const rowPaths = ROWS.map((_, r) => {
    const since = tt - (T_ROW + r * ROW_GAP);
    const active = !reset && since > 0 && since < 0.7;
    if (active) dots.push(pointAlong(rowRoute(r), ease(clamp(since / 0.7))));
    return { d: roundedPath(rowRoute(r)), stroke: active ? HOT : LINE };
  });
  const rows = ROWS.map(([name, tag], r) => {
    const at = T_ROW + r * ROW_GAP;
    const sent = !reset && tt >= at;
    const active = sent && tt < at + 0.7;
    return { name, tag, sent, active };
  });

  const hexProgress = clamp((tt - T_HEX) / 0.4);
  const pulse = hexProgress > 0 && hexProgress < 1 ? Math.sin(Math.PI * hexProgress) : 0;

  const sinceDelivery = tt - T_DLV;
  const deliveryLength = T_DLV_END - T_DLV;
  const delivering = !reset && sinceDelivery > 0 && sinceDelivery < deliveryLength;
  const harnessPaths = HARNESSES.map((_, j) => {
    if (delivering) dots.push(pointAlong(harnessRoute(j), ease(clamp(sinceDelivery / deliveryLength))));
    return { d: roundedPath(harnessRoute(j)), stroke: delivering ? HOT : LINE };
  });
  const delivered = !reset && tt >= T_DLV_END;
  const harnesses = HARNESSES.map((name, j) => ({
    name,
    running: !reset && tt >= T_RUN + j * 0.18,
    slots: SLOTS.map((letter, k) => ({ letter, color: ROUTE[j][k] === 'n' ? G : AMB })),
  }));
  const anyRunning = !reset && tt >= T_RUN;

  const extensions = EXTENSIONS.map((name, k) => {
    const at = T_EXT + k * EXT_GAP;
    return { name, on: !reset && tt >= at && tt < at + EXT_GAP, seen: !reset && tt >= at };
  });
  const extIndex =
    !reset && tt >= T_EXT && tt < T_EXT + 3 * EXT_GAP ? Math.floor((tt - T_EXT) / EXT_GAP) : -1;
  const sinceExt = extIndex >= 0 ? (tt - T_EXT) % EXT_GAP : 0;
  const extIn = extIndex >= 0 ? ease(clamp(sinceExt / 0.25)) : 0;
  const extOut = extIndex === 2 ? 1 - ease(clamp((sinceExt - (EXT_GAP - 0.25)) / 0.25)) : 1;

  const typed = `uze ${plugin}`;
  let command: string;
  let okOpacity: number;
  if (!reset && tt >= T_DONE) {
    command = `uze inspect ${plugin.split('@')[0]}`;
    okOpacity = clamp((tt - T_DONE) / 0.3);
  } else {
    command = typed.slice(0, Math.floor(clamp(tt / T_TYPE) * typed.length));
    okOpacity = delivered ? 1 : 0;
  }

  let status = 'one plugin, every agent';
  let statusColor = MUTED;
  if (!reset && tt >= T_ROW && tt < T_HEX) {
    status = 'resolving';
    statusColor = MUTED;
  } else if (!reset && tt >= T_HEX && tt < T_DLV_END) {
    status = 'delivering natively';
    statusColor = G;
  } else if (delivered) {
    status = 'delivered · 4 harnesses';
    statusColor = G;
  }

  return {
    rowPaths,
    rows,
    harnessPaths,
    harnesses,
    delivering,
    delivered,
    anyRunning,
    extensions,
    extIndex,
    extOpacity: extIn * extOut,
    extOffset: (1 - extIn) * 8,
    command,
    okOpacity,
    cursorOn: Math.floor(seconds * 2.5) % 2 === 0,
    status,
    statusColor,
    pulse,
    dots,
  };
}

function useClock(stageRef: React.RefObject<HTMLDivElement | null>) {
  const [seconds, setSeconds] = useState(STILL_AT);
  useEffect(() => {
    const element = stageRef.current;
    if (!element) return;
    const still = window.matchMedia('(prefers-reduced-motion: reduce)');
    let raf = 0;
    let visible = false;
    let origin = 0;
    let elapsed = 0;
    const tick = (now: number) => {
      setSeconds(elapsed + (now - origin) / 1000);
      raf = requestAnimationFrame(tick);
    };
    const start = () => {
      if (raf || still.matches || !visible) return;
      origin = performance.now();
      raf = requestAnimationFrame(tick);
    };
    const stop = () => {
      if (!raf) return;
      cancelAnimationFrame(raf);
      raf = 0;
      elapsed += (performance.now() - origin) / 1000;
    };
    const onMotionChange = () => {
      if (still.matches) {
        stop();
        setSeconds(STILL_AT);
      } else start();
    };
    // Off screen, it stops: a landing page scrolled past has no reason to
    // repaint at sixty frames a second.
    const observer = new IntersectionObserver(([entry]) => {
      visible = entry.isIntersecting;
      if (visible) start();
      else stop();
    });
    observer.observe(element);
    still.addEventListener('change', onMotionChange);
    return () => {
      stop();
      observer.disconnect();
      still.removeEventListener('change', onMotionChange);
    };
  }, [stageRef]);
  return seconds;
}

function useColumnWidth(wrapRef: React.RefObject<HTMLDivElement | null>) {
  const [width, setWidth] = useState(WIDE.width);
  useEffect(() => {
    const element = wrapRef.current;
    if (!element) return;
    const fit = () => setWidth(element.offsetWidth);
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    fit();
    return () => observer.disconnect();
  }, [wrapRef]);
  return width;
}

const panel: CSSProperties = {
  position: 'absolute',
  boxSizing: 'border-box',
  background: PANEL,
  borderRadius: 12,
  overflow: 'hidden',
  display: 'flex',
  flexDirection: 'column',
};
const nowrap: CSSProperties = { whiteSpace: 'nowrap' };

export function HeroIllustration({ plugin = 'review-kit@acme' }: { plugin?: string }) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const columnWidth = useColumnWidth(wrapRef);
  const layout = columnWidth < STACK_BELOW ? STACKED : WIDE;
  const scale = Math.min(1, columnWidth / layout.width);
  const seconds = useClock(wrapRef);
  const f = frame(seconds, plugin, layout);

  return (
    <div
      ref={wrapRef}
      role="img"
      aria-label={`uze ${plugin}: one plugin resolved from the Store, delivered natively to Claude Code, Codex, OpenCode and Antigravity, then read through the Spec, Architect and Code extensions.`}
      className="relative mx-auto w-full overflow-hidden font-mono"
      style={{ maxWidth: WIDE.width, height: Math.round(layout.height * scale) }}
    >
      <div
        aria-hidden
        style={{
          position: 'absolute',
          top: 0,
          left: `calc(50% - ${(layout.width * scale) / 2}px)`,
          width: layout.width,
          height: layout.height,
          transformOrigin: '0 0',
          transform: `scale(${scale})`,
          color: INK,
        }}
      >

        <svg
          width={layout.width}
          height={layout.height}
          style={{ position: 'absolute', inset: 0 }}
          fill="none"
          strokeWidth={1.5}
          strokeLinecap="round"
        >
          {[...f.rowPaths, ...f.harnessPaths].map((path, i) => (
            <path key={i} d={path.d} stroke={path.stroke} />
          ))}
        </svg>

        {/* The plugin, as the Store holds it. */}
        <div style={{ ...panel, left: layout.store[0], top: layout.store[1], width: 300, height: 400 }}>
          <div
            style={{
              ...nowrap,
              height: 52,
              boxSizing: 'border-box',
              display: 'flex',
              alignItems: 'center',
              gap: 10,
              padding: '0 18px',
              fontSize: 15,
            }}
          >
            <span style={{ color: G }}>$</span>
            <span style={{ display: 'flex', alignItems: 'center' }}>
              <span>{f.command}</span>
              <span style={{ color: G, opacity: f.cursorOn ? 1 : 0 }}>▍</span>
            </span>
            <span style={{ marginLeft: 'auto', color: G, fontSize: 13, opacity: f.okOpacity }}>✓</span>
          </div>
          <div style={{ flex: 1, padding: 16, display: 'flex', flexDirection: 'column', gap: 10 }}>
            {f.rows.map((row) => (
              <div
                key={row.name}
                style={{
                  ...nowrap,
                  height: 44,
                  boxSizing: 'border-box',
                  background: row.active ? CARD_ON : CARD,
                  borderRadius: 8,
                  display: 'flex',
                  alignItems: 'center',
                  gap: 12,
                  padding: '0 14px',
                  fontSize: 15,
                  color: row.sent ? INK : MUTED,
                }}
              >
                <span
                  style={{
                    width: 28,
                    height: 22,
                    borderRadius: 5,
                    background: row.sent ? G : DIM,
                    color: row.sent ? PAPER : MUTED,
                    fontSize: 11,
                    fontWeight: 700,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                  }}
                >
                  {row.tag}
                </span>
                <span>{row.name}</span>
                <span style={{ marginLeft: 'auto', color: G, fontSize: 13 }}>{row.sent ? '✓' : ''}</span>
              </div>
            ))}
          </div>
          <div
            style={{
              ...nowrap,
              height: 42,
              boxSizing: 'border-box',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              padding: '0 18px',
              fontSize: 12,
              color: MUTED,
            }}
          >
            <span>agents.yaml</span>
            <span>agents.lock</span>
          </div>
        </div>

        {/* uze, in the middle. */}
        <div
          style={{
            position: 'absolute',
            left: layout.hub[0],
            top: layout.hub[1],
            width: 128,
            height: 128,
            boxSizing: 'border-box',
            borderRadius: 28,
            background: CARD,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            transform: `scale(${1 + 0.06 * f.pulse})`,
            boxShadow: `0 0 ${44 * f.pulse}px ${mix(G, 35)}`,
            color: G,
          }}
        >
          <UzeMark width={79} height={79} />
        </div>
        <div
          style={{
            ...nowrap,
            position: 'absolute',
            left: layout.status[0],
            top: layout.status[1],
            width: 240,
            textAlign: 'center',
            fontSize: 13,
            color: f.statusColor,
            letterSpacing: '0.02em',
          }}
        >
          {f.status}
        </div>
        <div
          style={{
            ...nowrap,
            position: 'absolute',
            left: layout.status[0],
            top: layout.status[1] + 26,
            width: 240,
            display: 'flex',
            justifyContent: 'center',
            gap: 14,
            fontSize: 11,
          }}
        >
          {[
            [G, 'native'],
            [AMB, 'bridge · adapted'],
          ].map(([color, label]) => (
            <span key={label} style={{ display: 'flex', alignItems: 'center', gap: 6, color: MUTED }}>
              <span style={{ width: 8, height: 8, borderRadius: 2, background: color }} />
              {label}
            </span>
          ))}
        </div>

        {/* The workspace the harnesses run in. */}
        <div
          style={{
            ...panel,
            left: layout.workspace[0],
            top: layout.workspace[1],
            width: 320,
            height: 400,
          }}
        >
          <div
            style={{
              ...nowrap,
              height: 40,
              flexShrink: 0,
              boxSizing: 'border-box',
              display: 'flex',
              alignItems: 'center',
              gap: 10,
              padding: '0 16px',
              fontSize: 12,
              color: MUTED,
            }}
          >
            <span>workspace</span>
            <span style={{ marginLeft: 'auto', color: f.anyRunning ? G : MUTED }}>
              {f.anyRunning ? '4 running' : 'idle'}
            </span>
          </div>
          <div
            style={{
              flex: 1,
              padding: '12px 16px',
              display: 'flex',
              flexDirection: 'column',
              gap: 12,
              position: 'relative',
            }}
          >
            <ExtensionSheet index={f.extIndex} opacity={f.extOpacity} offset={f.extOffset} />
            {f.harnesses.map((harness) => (
              <div
                key={harness.name}
                style={{
                  ...nowrap,
                  height: 56,
                  boxSizing: 'border-box',
                  background: f.delivering ? CARD_ON : CARD,
                  borderRadius: 8,
                  display: 'flex',
                  flexDirection: 'column',
                  justifyContent: 'center',
                  padding: '0 14px',
                  minWidth: 0,
                  overflow: 'hidden',
                }}
              >
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    gap: 8,
                    fontSize: 15,
                  }}
                >
                  <span style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                    <span
                      style={{
                        width: 7,
                        height: 7,
                        borderRadius: '50%',
                        background: harness.running ? G : DIM,
                      }}
                    />
                    {harness.name}
                  </span>
                  <div style={{ display: 'flex', gap: 4 }}>
                    {harness.slots.map((slot) => (
                      <div
                        key={slot.letter}
                        style={{
                          width: 22,
                          height: 22,
                          boxSizing: 'border-box',
                          borderRadius: 5,
                          background: f.delivered ? slot.color : PANEL,
                          color: f.delivered ? PAPER : FAINT,
                          fontSize: 10,
                          fontWeight: 700,
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'center',
                        }}
                      >
                        {slot.letter}
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            ))}
          </div>
          <div
            style={{
              height: 56,
              flexShrink: 0,
              boxSizing: 'border-box',
              padding: '14px 16px',
              display: 'flex',
              gap: 8,
            }}
          >
            {f.extensions.map((extension) => (
              <div
                key={extension.name}
                style={{
                  ...nowrap,
                  flex: 1,
                  height: 26,
                  boxSizing: 'border-box',
                  borderRadius: 6,
                  background: extension.on ? G : extension.seen ? CARD_ON : CARD,
                  color: extension.on ? PAPER : extension.seen ? INK : MUTED,
                  fontSize: 12,
                  fontWeight: 700,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                {extension.name}
              </div>
            ))}
          </div>
        </div>

        {f.dots.map(([x, y], i) => (
          <div
            key={i}
            style={{
              position: 'absolute',
              left: x,
              top: y,
              width: 10,
              height: 10,
              margin: '-5px 0 0 -5px',
              borderRadius: '50%',
              background: G,
              boxShadow: `0 0 14px ${G}`,
            }}
          />
        ))}
      </div>
    </div>
  );
}

function ExtensionSheet({ index, opacity, offset }: { index: number; opacity: number; offset: number }) {
  return (
    <div
      style={{
        position: 'absolute',
        left: 16,
        right: 16,
        top: 12,
        bottom: 12,
        boxSizing: 'border-box',
        background: SHEET,
        borderRadius: 8,
        opacity,
        transform: `translateY(${offset}px)`,
        pointerEvents: 'none',
        overflow: 'hidden',
        display: 'flex',
        flexDirection: 'column',
        zIndex: 2,
      }}
    >
      <div
        style={{
          ...nowrap,
          height: 32,
          flexShrink: 0,
          boxSizing: 'border-box',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '0 12px',
          fontSize: 12,
        }}
      >
        <span style={{ color: G, fontWeight: 700 }}>{EXT_TITLE[index] ?? ''}</span>
        <span style={{ color: MUTED }}>{EXT_META[index] ?? ''}</span>
      </div>
      {index === 0 ? <SpecSheet /> : null}
      {index === 1 ? <ArchitectSheet /> : null}
      {index === 2 ? <CodeSheet /> : null}
    </div>
  );
}

function SpecSheet() {
  return (
    <div style={{ padding: 12, display: 'flex', flexDirection: 'column', gap: 9, fontSize: 12 }}>
      {SPEC_TASKS.map(([task, done]) => (
        <div
          key={task}
          style={{ ...nowrap, display: 'flex', alignItems: 'center', gap: 10, color: done ? INK : MUTED }}
        >
          <span
            style={{
              width: 14,
              height: 14,
              boxSizing: 'border-box',
              borderRadius: 3,
              background: done ? G : PANEL,
              color: PAPER,
              fontSize: 10,
              fontWeight: 700,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              flexShrink: 0,
            }}
          >
            {done ? '✓' : ''}
          </span>
          <span>{task}</span>
        </div>
      ))}
    </div>
  );
}

const box = (left: number, top: number, width: number, height: number, radius: number): CSSProperties => ({
  position: 'absolute',
  left,
  top,
  width,
  height,
  boxSizing: 'border-box',
  background: PANEL,
  borderRadius: radius,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
});

function ArchitectSheet() {
  return (
    <div style={{ flex: 1, position: 'relative' }}>
      <svg
        width={288}
        height={180}
        style={{ position: 'absolute', inset: 0 }}
        fill="none"
        stroke={SOFT}
        strokeWidth={1.5}
      >
        {[
          'M92 40 L108 40',
          'M180 40 L196 40',
          'M144 58 L144 74',
          'M144 74 L48 74 L48 92',
          'M144 74 L112 74 L112 92',
          'M144 74 L176 74 L176 92',
          'M144 74 L240 74 L240 92',
          'M144 130 L144 146',
        ].map((d) => (
          <path key={d} d={d} />
        ))}
      </svg>
      <div style={{ ...box(32, 24, 60, 32, 5), fontSize: 11, color: INK }}>CLI</div>
      <div
        style={{
          ...box(108, 24, 72, 32, 5),
          background: CARD_ON,
          fontSize: 11,
          color: G,
          fontWeight: 700,
        }}
      >
        Store
      </div>
      <div style={{ ...box(196, 24, 60, 32, 5), fontSize: 11, color: INK }}>TUI</div>
      {['claude', 'codex', 'opencode', 'antigrav'].map((name, i) => (
        <div key={name} style={{ ...box(22 + 64 * i, 92, 52, 26, 4), fontSize: 10, color: MUTED }}>
          {name}
        </div>
      ))}
      <div
        style={{
          position: 'absolute',
          left: 0,
          right: 0,
          bottom: 10,
          textAlign: 'center',
          fontSize: 11,
          color: MUTED,
        }}
      >
        C4 · container · Enter to walk in
      </div>
    </div>
  );
}

function CodeSheet() {
  return (
    <div style={{ padding: '10px 0', display: 'flex', flexDirection: 'column', fontSize: 11, lineHeight: 1.9 }}>
      {DIFF.map(([line, sign, text, background, color]) => (
        <div key={line} style={{ ...nowrap, display: 'flex', gap: 10, padding: '0 12px', background, color }}>
          <span style={{ width: 20, color: FAINT, textAlign: 'right', flexShrink: 0 }}>{line}</span>
          <span style={{ width: 10, flexShrink: 0 }}>{sign}</span>
          <span>{text}</span>
        </div>
      ))}
    </div>
  );
}
