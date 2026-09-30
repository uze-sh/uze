'use client';

import { useRef } from 'react';
import { UzeMark } from '@/components/uze-mark';
import {
  AMB,
  CARD,
  CARD_ON,
  DIM,
  FAINT,
  G,
  HOT,
  INK,
  LINE,
  MAX_WIDE_SCALE,
  MUTED,
  PANEL,
  PAPER,
  RouteDots,
  SHEET,
  STACKED_STAGE,
  STACK_BELOW,
  WIDE_STAGE,
  clamp,
  ease,
  mix,
  nowrap,
  panel,
  pointAlong,
  roundedPath,
  useClock,
  useColumnWidth,
  type Point,
} from '@/components/illustration-stage';
import matrix from '@/lib/harness-matrix.json';

// The landing page's package illustration: one plugin from a marketplace,
// resolved into the Store, delivered to four harnesses, and then shown at
// work in them: a skill invoked, its MCP server connected, its hook run.
// Drawn the way a terminal would draw it: rows of text on rules rather than
// cards, a prompt, and a fill only on the row that is active.

const ROWS: [string, string][] = [
  ['AGENTS.md', 'md'],
  ['skills/', 'S'],
  ['mcp/', 'M'],
  ['agents/', 'A'],
  ['hooks/', 'H'],
];
const HARNESSES = ['Claude Code', 'Codex', 'OpenCode', 'Antigravity'];
type Capability = 'context' | 'skills' | 'mcp' | 'agents' | 'hooks';
const SLOTS: [string, Capability][] = [
  ['md', 'context'],
  ['S', 'skills'],
  ['M', 'mcp'],
  ['A', 'agents'],
  ['H', 'hooks'],
];
// Each slot's colour is the route its harness really takes, read from the
// matrix the integrations generate (the same one the table below the fold
// is built from), so the picture cannot claim a native route the code
// stopped taking.
const routeOf = (harness: string, capability: Capability) =>
  matrix.harnesses.find((entry) => entry.name === harness)?.[capability] ?? 'native';
const colorOf = (route: string) => (route === 'native' ? G : AMB);
// What a delivered plugin looks like at work, one capability per tab. The
// hook tab answers the adapted hook route above in words.
const TABS = ['skills', 'mcp', 'hooks'];
const TAB_TITLE = ['skill · /git:commit', 'mcp · git', 'hook · PreToolUse'];
const TAB_META = ['Claude Code · 1 of 4', '4 connected', '4 ran'];

const TRANSCRIPT: [string, string][] = [
  ['> /git:commit', 'prompt'],
  ['● reading the staged diff', 'step'],
  ['● a Conventional Commit, from the diff', 'step'],
  ['fix(api): return 404 for unknown routes', 'result'],
  ['✓ committed 3f2a91c', 'done'],
];

// The script's clock, in seconds of one loop. Two commands, the way a reader
// would run them: the marketplace is added, then the plugin is installed
// from it.
const T_MARKET_TYPED = 1.0;
const T_PLUGIN = 1.8;
const T_PLUGIN_TYPED = 2.4;
const T_ROW = 2.6;
const ROW_GAP = 0.26;
const T_HEX = 4.1;
const T_DLV = 4.25;
const T_DLV_END = 4.9;
const T_RUN = 5.3;
const T_EXT = 6.4;
const EXT_GAP = 2.8;
const LOOP = 15.4;
// What a reader who asked for no motion sees: delivered everywhere, every
// route coloured, nothing over it.
const STILL_AT = 5.9;
// A route at rest: visible on either ground, so the flow reads before
// anything moves along it.
const REST = mix(MUTED, 45);

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
  ...WIDE_STAGE,
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
  ...STACKED_STAGE,
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

function frame(seconds: number, plugin: string, source: string, { rowRoute, harnessRoute }: Layout) {
  const tt = seconds % LOOP;
  const reset = tt > LOOP - 0.3;
  const dots: Point[] = [];

  const rowPaths = ROWS.map((_, r) => {
    const since = tt - (T_ROW + r * ROW_GAP);
    const active = !reset && since > 0 && since < 0.7;
    if (active) dots.push(pointAlong(rowRoute(r), ease(clamp(since / 0.7))));
    return { d: roundedPath(rowRoute(r)), stroke: active ? HOT : REST };
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
    return { d: roundedPath(harnessRoute(j)), stroke: delivering ? HOT : REST };
  });
  const delivered = !reset && tt >= T_DLV_END;
  const harnesses = HARNESSES.map((name, j) => ({
    name,
    ready: !reset && tt >= T_RUN + j * 0.18,
    slots: SLOTS.map(([letter, capability]) => ({
      letter,
      color: colorOf(routeOf(name, capability)),
    })),
  }));

  const tabs = TABS.map((name, k) => {
    const at = T_EXT + k * EXT_GAP;
    return {
      name,
      on: !reset && tt >= at && tt < at + EXT_GAP,
      seen: !reset && tt >= at,
    };
  });
  const extIndex = !reset && tt >= T_EXT && tt < T_EXT + 3 * EXT_GAP ? Math.floor((tt - T_EXT) / EXT_GAP) : -1;
  const sinceExt = extIndex >= 0 ? (tt - T_EXT) % EXT_GAP : 0;
  const extIn = extIndex >= 0 ? ease(clamp(sinceExt / 0.25)) : 0;
  const extOut = extIndex === 2 ? 1 - ease(clamp((sinceExt - (EXT_GAP - 0.25)) / 0.25)) : 1;

  const typing = (text: string, from: number, to: number) =>
    text.slice(0, Math.floor(clamp((tt - from) / (to - from)) * text.length));
  const addMarket = `uze market add ${source}`;
  let command: string;
  let okOpacity: number;
  if (reset) {
    command = '';
    okOpacity = 0;
  } else if (tt < T_PLUGIN) {
    command = typing(addMarket, 0, T_MARKET_TYPED);
    okOpacity = clamp((tt - T_MARKET_TYPED - 0.1) / 0.2);
  } else {
    command = typing(`uze ${plugin}`, T_PLUGIN, T_PLUGIN_TYPED);
    okOpacity = delivered ? 1 : 0;
  }

  let status = '~/.uze/store';
  let statusColor = MUTED;
  if (!reset && tt >= T_MARKET_TYPED && tt < T_PLUGIN) {
    status = `${source} added`;
    statusColor = MUTED;
  } else if (!reset && tt >= T_ROW && tt < T_HEX) {
    status = 'resolving';
    statusColor = MUTED;
  } else if (!reset && tt >= T_HEX && tt < T_DLV_END) {
    status = 'delivering';
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
    tabs,
    extIndex,
    extOpacity: extIn * extOut,
    extOffset: 0,
    command,
    okOpacity,
    cursorOn: Math.floor(seconds * 2.5) % 2 === 0,
    status,
    statusColor,
    pulse,
    dots,
  };
}

export function HeroIllustration({ plugin = 'git@ai', source = 'hiukky/ai' }: { plugin?: string; source?: string }) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const columnWidth = useColumnWidth(wrapRef, WIDE.width);
  const layout = columnWidth < STACK_BELOW ? STACKED : WIDE;
  const maxScale = layout === WIDE ? MAX_WIDE_SCALE : 1;
  const scale = Math.min(maxScale, columnWidth / layout.width);
  const seconds = useClock(wrapRef, STILL_AT);
  const f = frame(seconds, plugin, source, layout);

  return (
    <div
      ref={wrapRef}
      role="img"
      aria-label={`uze ${plugin}: one plugin from the ${source} marketplace, resolved into the Store and delivered natively to Claude Code, Codex, OpenCode and Antigravity, where its skill is invoked, its MCP server connects and its hook runs.`}
      className="relative mx-auto w-full overflow-hidden font-mono"
      style={{ height: Math.round(layout.height * scale) }}
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
        <div
          style={{
            ...panel,
            left: layout.store[0],
            top: layout.store[1],
            width: 300,
            height: 400,
          }}
        >
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
            <span
              style={{
                marginLeft: 'auto',
                color: G,
                fontSize: 13,
                opacity: f.okOpacity,
              }}
            >
              ✓
            </span>
          </div>
          <div
            style={{
              flex: 1,
              padding: '11px 16px',
              display: 'flex',
              flexDirection: 'column',
              gap: 0,
            }}
          >
            {f.rows.map((row) => (
              <div
                key={row.name}
                style={{
                  ...nowrap,
                  height: 54,
                  boxSizing: 'border-box',
                  background: row.active ? CARD_ON : 'transparent',
                  borderBottom: `1px solid ${mix(INK, 10)}`,
                  borderRadius: 0,
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
                    borderRadius: 0,
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
            <span>agents.yaml{f.delivered ? <span style={{ color: G }}> ✓</span> : null}</span>
            <span>agents.lock{f.delivered ? <span style={{ color: G }}> ✓</span> : null}</span>
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
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            transform: `scale(${1 + 0.08 * f.pulse})`,
            color: G,
          }}
        >
          <UzeMark width={64} height={64} />
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
            [AMB, 'adapted'],
          ].map(([color, label]) => (
            <span
              key={label}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 6,
                color: MUTED,
              }}
            >
              <span
                style={{
                  width: 8,
                  height: 8,
                  borderRadius: 0,
                  background: color,
                }}
              />
              {label}
            </span>
          ))}
        </div>

        {/* The harnesses on this machine, and what each one received. */}
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
            <span>harnesses</span>
            <span style={{ marginLeft: 'auto', color: f.delivered ? G : MUTED }}>
              delivered {f.delivered ? '4/4' : '0/4'}
            </span>
          </div>
          <div
            style={{
              flex: 1,
              padding: '6px 16px',
              display: 'flex',
              flexDirection: 'column',
              gap: 0,
              position: 'relative',
            }}
          >
            <CapabilitySheet index={f.extIndex} opacity={f.extOpacity} offset={f.extOffset} />
            {f.harnesses.map((harness) => (
              <div
                key={harness.name}
                style={{
                  ...nowrap,
                  height: 68,
                  boxSizing: 'border-box',
                  background: f.delivering ? CARD_ON : 'transparent',
                  borderBottom: `1px solid ${mix(INK, 10)}`,
                  borderRadius: 0,
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
                        background: harness.ready ? G : DIM,
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
                          borderRadius: 0,
                          background: f.delivered ? slot.color : 'transparent',
                          border: f.delivered ? 'none' : `1px solid ${mix(INK, 16)}`,
                          color: f.delivered ? PAPER : 'transparent',
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
            {f.tabs.map((extension) => (
              <div
                key={extension.name}
                style={{
                  ...nowrap,
                  flex: 1,
                  height: 26,
                  boxSizing: 'border-box',
                  borderRadius: 0,
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

        <RouteDots dots={f.dots} />
      </div>
    </div>
  );
}

function CapabilitySheet({ index, opacity, offset }: { index: number; opacity: number; offset: number }) {
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
        borderRadius: 0,
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
        <span style={{ color: G, fontWeight: 700 }}>{TAB_TITLE[index] ?? ''}</span>
        <span style={{ color: MUTED }}>{TAB_META[index] ?? ''}</span>
      </div>
      {index === 0 ? <SkillSheet /> : null}
      {index === 1 ? <McpSheet /> : null}
      {index === 2 ? <HookSheet /> : null}
    </div>
  );
}

function SkillSheet() {
  const colorOf = { prompt: INK, step: MUTED, result: G, done: G };
  return (
    <div
      style={{
        padding: 12,
        display: 'flex',
        flexDirection: 'column',
        gap: 10,
        fontSize: 12,
      }}
    >
      {TRANSCRIPT.map(([line, kind]) => (
        <div
          key={line}
          style={{
            ...nowrap,
            color: colorOf[kind as keyof typeof colorOf],
            fontWeight: kind === 'prompt' || kind === 'result' ? 700 : 400,
          }}
        >
          {line}
        </div>
      ))}
    </div>
  );
}

// One row per harness, the same four the cards under this sheet name.
function HarnessRows({ status }: { status: (j: number) => [string, string] }) {
  return (
    <div
      style={{
        padding: 12,
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        fontSize: 12,
      }}
    >
      {HARNESSES.map((name, j) => {
        const [label, color] = status(j);
        return (
          <div
            key={name}
            style={{
              ...nowrap,
              height: 30,
              boxSizing: 'border-box',
              borderRadius: 0,
              background: PANEL,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              padding: '0 10px',
            }}
          >
            <span
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                color: INK,
              }}
            >
              <span
                style={{
                  width: 6,
                  height: 6,
                  borderRadius: '50%',
                  background: color,
                }}
              />
              {name}
            </span>
            <span style={{ color }}>{label}</span>
          </div>
        );
      })}
    </div>
  );
}

function McpSheet() {
  return <HarnessRows status={() => ['connected · 3 tools', G]} />;
}

function HookSheet() {
  return (
    <HarnessRows
      status={(j) => {
        const route = routeOf(HARNESSES[j], 'hooks');
        return [`${route} · allowed`, colorOf(route)];
      }}
    />
  );
}
