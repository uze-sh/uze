'use client';

import { useRef, type CSSProperties, type ReactNode } from 'react';
import {
  AMB,
  CARD,
  CARD_ON,
  G,
  INK,
  LINE,
  MAX_WIDE_SCALE,
  MUTED,
  PANEL,
  PAPER,
  RED,
  STACK_BELOW,
  clamp,
  mix,
  nowrap,
  useClock,
  useColumnWidth,
} from '@/components/illustration-stage';

// The landing page's workspace illustration: the workspace's shape, drawn
// rather than recorded. Spaces down the side, each holding its agents; one
// thing at a time in the main area, either the selected agent's session in
// its own checkout or an extension over the project (the plan, the
// architecture, the code). It is a picture of how the product is arranged,
// so nothing is on it that the product does not do.

type AgentRow = {
  name: string;
  harness: string;
  checkout: string;
  prompt: string;
  activity: string[];
};

const AGENTS: AgentRow[] = [
  {
    name: 'ping',
    harness: 'claude',
    checkout: '.worktrees/k2x9fq · agent/k2x9fq',
    prompt: 'return 404 for routes nobody declared',
    activity: ['+ Thought · 1.2s', '→ Explored · 3 reads', '● Edited src/router.rs', '✓ Committed a41e2b7'],
  },
  {
    name: 'docs',
    harness: 'opencode',
    checkout: '.worktrees/07lmdo · agent/0kh2yu',
    prompt: 'add a Routes section to README.md',
    activity: ['+ Thought · 1.5s', '→ Explored · 2 reads', '● Writing README.md'],
  },
];
const TIMELINE: [string, string][] = [
  ['docs(openspec): plan rate limiting', '27h'],
  ['feat(config): read settings', '4d'],
  ['feat(api): add the health route', '7d'],
  ['chore: initial commit', '9d'],
];
const EXTENSIONS = ['spec', 'arch', 'code'] as const;

// The script: what the main area shows, and when.
type View = 'agent-0' | 'agent-1' | (typeof EXTENSIONS)[number];
const SCHEDULE: [number, View][] = [
  [0, 'agent-0'],
  [5.2, 'agent-1'],
  [9.2, 'spec'],
  [12.8, 'arch'],
  [16.4, 'code'],
  [19.8, 'agent-0'],
];
const LOOP = 23;
const T_NEW = 1.6; // the second agent is placed
const T_DONE = 4.2; // the first one commits
const T_LANDED = 20.2; // and its work lands on main
const activityAt = (agent: number, i: number) => (agent === 0 ? 0.6 : 5.6) + i * 0.8;
// What a reader who asked for no motion sees: both agents placed, the first
// one's work committed, the second at work.
const STILL_AT = 7.6;

function frame(seconds: number) {
  const tt = seconds % LOOP;
  let index = 0;
  for (let i = 0; i < SCHEDULE.length; i++) if (tt >= SCHEDULE[i][0]) index = i;
  const [since, view] = SCHEDULE[index];
  const until = index + 1 < SCHEDULE.length ? SCHEDULE[index + 1][0] : LOOP;
  const fade = Math.min(clamp((tt - since) / 0.25), clamp((until - tt) / 0.25));
  // An extension shows the project from the checkout of whoever is selected;
  // in this story that is still the second agent when they open.
  const selected = view === 'agent-0' ? 0 : 1;
  return {
    tt,
    view,
    fade,
    // The whole frame fades out and back in at the loop, so the second agent
    // is not seen to vanish.
    edge: Math.min(clamp(tt / 0.3), clamp((LOOP - tt) / 0.3)),
    selected,
    landed: tt >= T_LANDED,
    placed: tt >= T_NEW ? 2 : 1,
    done: tt >= T_DONE,
    shown: (agent: number) => AGENTS[agent].activity.filter((_, i) => tt >= activityAt(agent, i)).length,
    spin: Math.floor(seconds * 8) % 2 === 0,
  };
}
type Frame = ReturnType<typeof frame>;

const WIDE = { width: 1200, height: 500, stacked: false };
const STACKED = { width: 380, height: 840, stacked: true };

export function WorkspaceIllustration() {
  const wrapRef = useRef<HTMLDivElement>(null);
  const columnWidth = useColumnWidth(wrapRef, WIDE.width);
  const layout = columnWidth < STACK_BELOW ? STACKED : WIDE;
  const maxScale = layout === WIDE ? MAX_WIDE_SCALE : 1;
  const scale = Math.min(maxScale, columnWidth / layout.width);
  const seconds = useClock(wrapRef, STILL_AT);
  const f = frame(seconds);

  return (
    <div
      ref={wrapRef}
      role="img"
      aria-label="The uze workspace: spaces down the side, each holding its agents, and the selected agent's session in the main area, running in a checkout of its own. The spec, arch and code extensions open over the same area to show the project's plan, its architecture and its code."
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
          opacity: f.edge,
          padding: layout.stacked ? '10px 0' : '20px 40px',
          boxSizing: 'border-box',
        }}
      >
        {/* The terminal you already have, with the command that opened the
            workspace in it: uze is not an app window of its own. */}
        <div
          style={{
            height: '100%',
            boxSizing: 'border-box',
            background: PANEL,
            border: `1px solid ${mix(INK, 16)}`,
            overflow: 'hidden',
            display: 'flex',
            flexDirection: 'column',
          }}
        >
          <div
            style={{
              ...row,
              height: 30,
              flexShrink: 0,
              padding: '0 12px',
              fontSize: 11,
              color: MUTED,
              borderBottom: `1px solid ${mix(INK, 16)}`,
            }}
          >
            <span style={{ color: G }}>~/api $</span>
            <span style={{ color: INK }}>uze workspace</span>
          </div>
          <div
            style={{
              flex: 1,
              minHeight: 0,
              display: 'flex',
              flexDirection: layout.stacked ? 'column' : 'row',
            }}
          >
            <Sidebar f={f} stacked={layout.stacked} />
            <div
              style={{
                flex: 1,
                minWidth: 0,
                minHeight: 0,
                display: 'flex',
                flexDirection: 'column',
                position: 'relative',
              }}
            >
              <Strip f={f} stacked={layout.stacked} />
              <div
                style={{
                  flex: 1,
                  minHeight: 0,
                  padding: 16,
                  opacity: f.fade,
                  display: 'flex',
                }}
              >
                {f.view === 'agent-0' ? <Session f={f} agent={0} /> : null}
                {f.view === 'agent-1' ? <Session f={f} agent={1} /> : null}
                {f.view === 'spec' ? <SpecView stacked={layout.stacked} /> : null}
                {f.view === 'arch' ? <ArchView /> : null}
                {f.view === 'code' ? <CodeView stacked={layout.stacked} /> : null}
              </div>
              <Hints view={f.view} />
              {f.landed ? <Toast /> : null}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

const row: CSSProperties = {
  ...nowrap,
  display: 'flex',
  alignItems: 'center',
  gap: 8,
};

// Spaces, each holding its agents, and the checkout's history under them.
function Sidebar({ f, stacked }: { f: Frame; stacked: boolean }) {
  return (
    <div
      style={{
        width: stacked ? 'auto' : 280,
        flexShrink: 0,
        boxSizing: 'border-box',
        display: 'flex',
        flexDirection: 'column',
        borderRight: stacked ? 'none' : `1px solid ${LINE}`,
        borderBottom: stacked ? `1px solid ${LINE}` : 'none',
        fontSize: 12,
      }}
    >
      <div style={{ ...row, height: 44, padding: '0 16px', background: CARD }}>
        <span style={{ color: MUTED, fontWeight: 700 }}>work</span>
        <span style={{ marginLeft: 'auto', color: G, fontWeight: 700 }}>+ space</span>
      </div>
      <div
        style={{
          padding: '12px 10px 0',
          display: 'flex',
          flexDirection: 'column',
          gap: 4,
        }}
      >
        <div
          style={{
            ...row,
            height: 26,
            padding: '0 8px',
            borderRadius: 0,
            background: CARD,
            fontSize: 13,
          }}
        >
          <span style={{ color: MUTED }}>▾</span>
          <span style={{ fontWeight: 700 }}>api</span>
          <span
            style={{
              marginLeft: 'auto',
              color: f.tt >= T_NEW - 0.6 && f.tt < T_NEW ? G : AMB,
              fontWeight: 700,
            }}
          >
            ✦ new
          </span>
        </div>
        {AGENTS.slice(0, f.placed).map((agent, i) => (
          <AgentEntry key={agent.name} agent={agent} selected={f.selected === i} state={stateOf(f, i)} spin={f.spin} />
        ))}
        <div
          style={{
            ...row,
            height: 26,
            padding: '0 8px',
            color: MUTED,
            fontSize: 13,
          }}
        >
          <span>▸</span>
          <span>web</span>
          <span style={{ marginLeft: 'auto', fontSize: 12 }}>1 agent</span>
        </div>
      </div>
      {stacked ? null : (
        <div
          style={{
            marginTop: 'auto',
            padding: '0 10px 14px',
            display: 'flex',
            flexDirection: 'column',
            gap: 7,
          }}
        >
          <div
            style={{
              ...row,
              height: 26,
              padding: '0 8px',
              borderRadius: 0,
              background: CARD,
            }}
          >
            <span style={{ color: MUTED }}>▾</span>
            <span style={{ fontWeight: 700 }}>timeline</span>
            <span style={{ marginLeft: 'auto', color: MUTED }}>{timelineBranch(f)}</span>
          </div>
          {timelineOf(f).map(([subject, age], i) => {
            const fresh = i === 0 && age === 'now';
            return (
              <div
                key={subject}
                style={{
                  ...row,
                  padding: '0 8px',
                  color: fresh ? INK : MUTED,
                }}
              >
                <span
                  style={{
                    width: 7,
                    height: 7,
                    borderRadius: '50%',
                    background: fresh ? G : AMB,
                  }}
                />
                <span style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>{subject}</span>
                <span style={{ marginLeft: 'auto', color: MUTED }}>{age}</span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

type State = 'working' | 'done' | 'landed';
const stateOf = (f: Frame, agent: number): State =>
  agent !== 0 ? 'working' : f.landed ? 'landed' : f.done ? 'done' : 'working';

// The timeline is the selected checkout's branch: the first agent's commit is
// on its branch, and on main once it has landed; the second agent's branch
// has not got it.
const timelineBranch = (f: Frame) =>
  f.selected === 0 ? (f.landed ? 'main' : AGENTS[0].checkout.split(' · ')[1]) : AGENTS[1].checkout.split(' · ')[1];
const timelineOf = (f: Frame): [string, string][] =>
  f.selected === 0 && f.done ? [['fix(api): return 404 for unknown routes', 'now'], ...TIMELINE] : TIMELINE;

function AgentEntry({
  agent,
  selected,
  state,
  spin,
}: {
  agent: AgentRow;
  selected: boolean;
  state: State;
  spin: boolean;
}) {
  return (
    <div
      style={{
        padding: '5px 8px 5px 22px',
        borderRadius: 0,
        background: selected ? CARD_ON : 'transparent',
        boxShadow: selected ? `inset 2px 0 0 ${G}` : 'none',
        display: 'flex',
        flexDirection: 'column',
        gap: 2,
      }}
    >
      <div style={{ ...row, fontSize: 13 }}>
        <span style={{ width: 12, color: G, textAlign: 'center' }}>
          {state === 'working' ? (spin ? '⁘' : '⁙') : '✓'}
        </span>
        <span style={{ fontWeight: selected ? 700 : 400 }}>{agent.name}</span>
        {state === 'done' ? <span style={{ marginLeft: 'auto', color: G }}>±</span> : null}
      </div>
      <div style={{ paddingLeft: 20, color: selected ? AMB : MUTED }}>{agent.harness}</div>
    </div>
  );
}

// The strip over the main area: the selected agent's tab, a new one, and
// the extensions.
function Strip({ f, stacked }: { f: Frame; stacked: boolean }) {
  const extension = EXTENSIONS.find((name) => name === f.view);
  return (
    <div
      style={{
        ...row,
        height: 44,
        flexShrink: 0,
        padding: '0 14px',
        background: CARD,
        fontSize: 12,
      }}
    >
      <span
        style={{
          ...row,
          height: 26,
          padding: '0 10px',
          borderRadius: 0,
          background: extension ? 'transparent' : CARD_ON,
          color: extension ? MUTED : INK,
          fontWeight: 700,
        }}
      >
        <span style={{ color: G }}>✦</span>
        {AGENTS[f.selected].name}
      </span>
      <span
        style={{
          ...row,
          height: 26,
          padding: '0 9px',
          borderRadius: 0,
          background: CARD_ON,
          color: MUTED,
        }}
      >
        +
      </span>
      <span style={{ marginLeft: 'auto', ...row, gap: 6 }}>
        {f.done && !f.landed && !stacked ? (
          <span style={{ marginRight: 8 }}>
            <span style={{ color: G }}>+12</span> <span style={{ color: RED }}>−3</span>
          </span>
        ) : null}
        {EXTENSIONS.map((name) => (
          <span
            key={name}
            style={{
              ...row,
              height: 26,
              padding: '0 10px',
              borderRadius: 0,
              fontWeight: 700,
              background: extension === name ? INK : CARD_ON,
              color: extension === name ? PAPER : MUTED,
            }}
          >
            {name}
          </span>
        ))}
      </span>
    </div>
  );
}

function Card({ children, style }: { children: ReactNode; style?: CSSProperties }) {
  return <div style={{ borderRadius: 0, background: CARD, ...style }}>{children}</div>;
}

// The agent's own harness, in its own checkout: what it was asked, what it
// is doing, and where it is doing it.
function Session({ f, agent }: { f: Frame; agent: number }) {
  const a = AGENTS[agent];
  const shown = f.shown(agent);
  return (
    <div
      style={{
        flex: 1,
        minWidth: 0,
        display: 'flex',
        flexDirection: 'column',
        gap: 14,
        fontSize: 13,
      }}
    >
      <Card
        style={{
          padding: '14px 16px',
          boxShadow: `inset 3px 0 0 ${mix(G, 70)}`,
        }}
      >
        <div style={{ ...nowrap, overflow: 'hidden', textOverflow: 'ellipsis' }}>{a.prompt}</div>
      </Card>
      <div
        style={{
          padding: '0 16px',
          display: 'flex',
          flexDirection: 'column',
          gap: 11,
        }}
      >
        {a.activity.slice(0, shown).map((line) => (
          <div
            key={line}
            style={{
              ...nowrap,
              color: line.startsWith('✓') ? G : line.startsWith('+') ? AMB : MUTED,
            }}
          >
            {line}
          </div>
        ))}
      </div>
      <div style={{ marginTop: 'auto', display: 'flex', flexDirection: 'column', gap: 8 }}>
        <Card style={{ padding: '12px 16px', display: 'flex', flexDirection: 'column', gap: 8 }}>
          {a.harness === 'claude' ? (
            <>
              <span style={{ color: MUTED }}>&gt;</span>
              <span style={{ fontSize: 11, color: MUTED }}>⏵⏵ accept edits on</span>
            </>
          ) : (
            <>
              <span style={{ color: MUTED }}>Ask anything…</span>
              <span style={{ fontSize: 11 }}>
                <span style={{ color: G }}>Build</span> <span style={{ color: MUTED }}>· {a.harness}</span>
              </span>
            </>
          )}
        </Card>
        <span style={{ ...row, fontSize: 11, color: MUTED, padding: '0 4px' }}>
          <span style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>{a.checkout}</span>
          <span style={{ marginLeft: 'auto', color: AMB }}>{a.harness}</span>
        </span>
      </div>
    </div>
  );
}

// The keys that act here, on the last line, the way the workspace says them.
const HINTS: Record<View, [string, string][]> = {
  'agent-0': [
    ['alt+n', 'new agent'],
    ['alt+down', 'next agent'],
    ['ctrl+o', 'manage'],
    ['ctrl+q', 'quit'],
  ],
  'agent-1': [
    ['alt+n', 'new agent'],
    ['alt+down', 'next agent'],
    ['ctrl+o', 'manage'],
    ['ctrl+q', 'quit'],
  ],
  spec: [
    ['down', 'next'],
    ['right', 'expand'],
    ['enter', 'open'],
    ['esc', 'close'],
  ],
  arch: [
    ['down', 'next'],
    ['enter', 'walk in'],
    ['esc', 'close'],
  ],
  code: [
    ['down', 'next'],
    ['enter', 'open'],
    ['m', 'map'],
    ['esc', 'close'],
  ],
};

function Hints({ view }: { view: View }) {
  return (
    <div
      style={{
        ...row,
        height: 30,
        flexShrink: 0,
        padding: '0 16px',
        fontSize: 11,
        gap: 14,
        borderTop: `1px solid ${mix(INK, 10)}`,
      }}
    >
      {HINTS[view].map(([key, action]) => (
        <span key={key} style={{ color: MUTED }}>
          <span style={{ color: G, fontWeight: 700 }}>{key}</span> {action}
        </span>
      ))}
    </div>
  );
}

// An outcome, top right, the way the workspace reports one: a title and what
// it is about.
function Toast() {
  return (
    <div
      style={{
        position: 'absolute',
        top: 56,
        right: 14,
        width: 280,
        padding: '10px 12px',
        background: PANEL,
        border: `1px solid ${mix(INK, 20)}`,
        display: 'flex',
        flexDirection: 'column',
        gap: 4,
        fontSize: 12,
        zIndex: 3,
      }}
    >
      <span style={{ ...row }}>
        <span style={{ color: G }}>✓</span>
        <span style={{ fontWeight: 700 }}>ping landed on main</span>
      </span>
      <span style={{ color: MUTED, paddingLeft: 16 }}>rebased · checks passed · a41e2b7</span>
    </div>
  );
}

function Tabs({ names, active }: { names: string[]; active: number }) {
  return (
    <div style={{ ...row, gap: 4, fontSize: 12 }}>
      {names.map((name, i) => (
        <span
          key={name}
          style={{
            padding: '4px 9px',
            borderRadius: 0,
            background: i === active ? CARD_ON : 'transparent',
            color: i === active ? INK : MUTED,
            fontWeight: i === active ? 700 : 400,
          }}
        >
          {name}
        </span>
      ))}
    </div>
  );
}

// The project's plan: its changes, and how far each has got.
function SpecView({ stacked }: { stacked: boolean }) {
  const tasks: [string, boolean][] = [
    ['1.1 Token bucket with a refill clock', true],
    ['1.2 One bucket per client address', true],
    ['1.3 Read the budget from config.toml', true],
    ['2.1 Ask the limiter before dispatching', false],
    ['2.2 Answer 429 with Retry-After', false],
  ];
  return (
    <div
      style={{
        flex: 1,
        minWidth: 0,
        display: 'flex',
        flexDirection: 'column',
        gap: 12,
        fontSize: 12,
      }}
    >
      <Tabs names={['Changes', 'Specs', 'Archive']} active={0} />
      <div
        style={{
          flex: 1,
          display: 'flex',
          flexDirection: stacked ? 'column' : 'row',
          gap: 16,
        }}
      >
        <div
          style={{
            width: stacked ? 'auto' : 250,
            display: 'flex',
            flexDirection: 'column',
            gap: 7,
          }}
        >
          <div style={{ color: MUTED, fontWeight: 700, letterSpacing: '0.04em' }}>IN PROGRESS 2</div>
          <div style={{ ...row, color: MUTED }}>
            add-config-reload<span style={{ marginLeft: 'auto' }}>1/4</span>
          </div>
          <div
            style={{
              ...row,
              padding: '3px 6px',
              margin: '0 -6px',
              borderRadius: 0,
              background: CARD_ON,
              fontWeight: 700,
            }}
          >
            add-rate-limiting
            <span style={{ marginLeft: 'auto', color: MUTED, fontWeight: 400 }}>3/7</span>
          </div>
          <div style={{ paddingLeft: 14, color: MUTED }}>proposal</div>
          <div style={{ paddingLeft: 14, color: INK }}>tasks</div>
          <div
            style={{
              marginTop: 6,
              color: MUTED,
              fontWeight: 700,
              letterSpacing: '0.04em',
            }}
          >
            READY TO ARCHIVE 1
          </div>
          <div style={{ ...row, color: MUTED }}>
            add-request-logging<span style={{ marginLeft: 'auto' }}>3/3</span>
          </div>
        </div>
        <Card
          style={{
            flex: 1,
            minWidth: 0,
            padding: '12px 16px',
            display: 'flex',
            flexDirection: 'column',
            gap: 9,
          }}
        >
          <span style={{ color: MUTED }}>add-rate-limiting/tasks.md</span>
          {tasks.map(([task, done]) => (
            <div key={task} style={{ ...row, color: done ? INK : MUTED }}>
              <span style={{ color: done ? G : MUTED }}>{done ? '[x]' : '[ ]'}</span>
              <span style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>{task}</span>
            </div>
          ))}
        </Card>
      </div>
    </div>
  );
}

// What the project wrote down about itself, drawn in cells: a box is a level
// you can walk into.
function ArchView() {
  const cell = (label: string, detail: string, lit = false): ReactNode => (
    <div
      style={{
        padding: '9px 12px',
        borderRadius: 0,
        border: `1px solid ${lit ? G : mix(INK, 25)}`,
        background: lit ? mix(G, 12, PANEL) : PANEL,
        display: 'flex',
        flexDirection: 'column',
        gap: 3,
      }}
    >
      <span style={{ fontWeight: 700, color: lit ? G : INK }}>{label}</span>
      <span style={{ color: MUTED, fontSize: 11 }}>{detail}</span>
    </div>
  );
  return (
    <div
      style={{
        flex: 1,
        minWidth: 0,
        display: 'flex',
        flexDirection: 'column',
        gap: 12,
        fontSize: 12,
      }}
    >
      <div style={{ ...row }}>
        <span style={{ color: MUTED }}>docs/architecture/containers.mmd</span>
        <span style={{ marginLeft: 'auto', color: MUTED }}>C4</span>
      </div>
      <div
        style={{
          flex: 1,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          gap: 18,
        }}
      >
        {cell('client', 'any HTTP caller')}
        <span style={{ color: MUTED }}>→</span>
        <div
          style={{
            padding: 12,
            borderRadius: 0,
            border: `1px dashed ${mix(INK, 30)}`,
            display: 'flex',
            flexDirection: 'column',
            gap: 10,
          }}
        >
          <span style={{ color: MUTED }}>api · service</span>
          <div style={{ display: 'flex', gap: 10 }}>
            {cell('router', 'dispatches')}
            {cell('limiter', 'token bucket', true)}
          </div>
          {cell('health', 'GET /health')}
        </div>
        <span style={{ color: MUTED }}>→</span>
        {cell('config.toml', '[limits]')}
      </div>
    </div>
  );
}

// The checkout as code: its files, sized by lines, the changed ones marked.
function CodeView({ stacked }: { stacked: boolean }) {
  const tile = (name: string, detail: string, changed: boolean, grow: number): ReactNode => (
    <div
      key={name}
      style={{
        flex: grow,
        minWidth: 0,
        minHeight: 0,
        padding: '8px 10px',
        borderRadius: 0,
        border: `1px solid ${changed ? RED : mix(INK, 25)}`,
        display: 'flex',
        flexDirection: 'column',
        gap: 3,
      }}
    >
      <span style={{ ...nowrap, fontWeight: 700, color: changed ? RED : INK }}>{name}</span>
      <span style={{ ...nowrap, color: MUTED, fontSize: 11 }}>{detail}</span>
    </div>
  );
  return (
    <div
      style={{
        flex: 1,
        minWidth: 0,
        display: 'flex',
        flexDirection: 'column',
        gap: 12,
        fontSize: 12,
      }}
    >
      <div style={{ ...row }}>
        <Tabs names={['Files', 'Map', 'Changes']} active={1} />
        <span style={{ marginLeft: 'auto', color: MUTED }}>25 files · 217 lines</span>
      </div>
      <div
        style={{
          flex: 1,
          minHeight: 0,
          display: 'flex',
          flexDirection: stacked ? 'column' : 'row',
          gap: 8,
        }}
      >
        {tile('openspec/', '106 lines · 12 files', false, 5)}
        <div
          style={{
            flex: 5,
            minWidth: 0,
            display: 'flex',
            flexDirection: 'column',
            gap: 8,
          }}
        >
          {tile('src/', '26 lines · 4 files', true, 3)}
          <div style={{ flex: 2, minHeight: 0, display: 'flex', gap: 8 }}>
            {tile('docs/', '54 lines', false, 3)}
            {tile('AGENTS.md', '14 lines', false, 2)}
          </div>
        </div>
      </div>
    </div>
  );
}
