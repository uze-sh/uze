'use client';

import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { WORKSPACE_LENGTH, Workspace } from '@/components/home/workspace';

// One terminal session, scripted: a marketplace is added, one plugin is
// installed and the stream splits into four lanes, one per agent, each
// printing what it received in its own form; the project file that now
// records it; then `uze workspace` takes the screen, with three agents each
// on a branch in a checkout of its own, and spec, arch and code open in turn
// on the first one to finish. The commands and the report lines
// are the CLI's own (src/progress.rs, src/cli/report.rs), the manifest keys
// are docs/reference/project-files.mdx's, and the workspace is drawn after
// its recorded frame.

type Tone = 'ink' | 'muted' | 'success' | 'warn' | 'title' | 'key';
type Span = { text: string; tone?: Tone };
type Row = Span[];

type Lane = { name: string; rows: Row[] };

type Event =
  | { kind: 'prompt'; at: number; text: string; charMs: number; done: number }
  | { kind: 'row'; at: number; row: Row }
  | { kind: 'spin'; at: number; until: number; text: string }
  | { kind: 'lanes'; at: number; lanes: Lane[]; laneStagger: number; rowStagger: number }
  | { kind: 'screen'; at: number };

type Chapter = { label: string; at: number; end: number };

const VERSION = process.env.NEXT_PUBLIC_UZE_VERSION ?? '1.0.0';
const CHAR_MS = 42;
const ENTER_MS = 320;
// Output arrives the way a terminal flushes it, all but at once: a row every
// 70ms read as text being typed out. The time to read it is a pause after
// the block, not a slow reveal of it.
const ROW_MS = 12;
const TICK_MS = 33;
const SPINNER = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

const ok = (name: string, detail?: string): Row => [
  { text: '✓ ', tone: 'success' },
  { text: name, tone: 'ink' },
  ...(detail ? [{ text: '  ' + detail, tone: 'muted' as Tone }] : []),
];
const adapted = (name: string, detail: string): Row => [
  { text: '≈ ', tone: 'warn' },
  { text: name, tone: 'ink' },
  { text: '  ' + detail, tone: 'muted' },
];
const none = (name: string, detail: string): Row => [
  { text: '· ', tone: 'muted' },
  { text: name, tone: 'muted' },
  { text: '  ' + detail, tone: 'muted' },
];

// What each agent receives of the git plugin, in the route the integration
// really takes (web/lib/harness-matrix.json): OpenCode has no plugin
// envelope and runs hooks through the bridge uze owns; Codex reads a
// subagent as TOML.
const delivered: Lane[] = [
  {
    name: 'Claude Code',
    rows: [ok('plugin'), ok('skill commit'), ok('skill pr'), ok('agent reviewer'), ok('hook pre-push')],
  },
  {
    name: 'Codex',
    rows: [ok('plugin'), ok('skill commit'), ok('skill pr'), ok('agent reviewer', 'toml'), ok('hook pre-push')],
  },
  {
    name: 'OpenCode',
    rows: [none('plugin', 'none'), ok('skill commit'), ok('skill pr'), ok('agent reviewer'), adapted('hook pre-push', 'adapted')],
  },
  {
    name: 'Antigravity',
    rows: [ok('plugin'), ok('skill commit'), ok('skill pr'), ok('agent reviewer'), ok('hook pre-push')],
  },
];

const yaml = (key: string, value?: string, indent = 0): Row =>
  value === undefined
    ? [{ text: ' '.repeat(indent) + key + ':', tone: 'key' }]
    : [{ text: ' '.repeat(indent) + key + ': ', tone: 'key' }, { text: value, tone: 'ink' }];

// This repository's own agents.yaml, trimmed to the keys the page talks about.
const manifest: Row[] = [
  yaml('worktrees'),
  yaml('default', 'isolated', 2),
  yaml('completion', 'pr', 2),
  yaml('branch', 'conventional', 2),
  [],
  yaml('marketplaces'),
  yaml('ai', undefined, 2),
  yaml('git', 'https://github.com/hiukky/ai', 4),
  yaml('plugins', undefined, 4),
  [
    { text: '      - ', tone: 'key' },
    { text: 'git', tone: 'ink' },
  ],
];

function report(verb: string): Row {
  return [
    { text: `uze ${verb}`, tone: 'title' },
    { text: `  v${VERSION}`, tone: 'muted' },
  ];
}

function build() {
  const events: Event[] = [];
  const chapters: Chapter[] = [];
  let t = 500;

  const prompt = (text: string) => {
    const done = t + text.length * CHAR_MS + ENTER_MS;
    events.push({ kind: 'prompt', at: t, text, charMs: CHAR_MS, done });
    t = done;
  };
  const spin = (text: string, ms: number) => {
    events.push({ kind: 'spin', at: t, until: t + ms, text });
    t += ms;
  };
  const rows = (list: Row[], step = ROW_MS) => {
    for (const row of list) {
      events.push({ kind: 'row', at: t, row });
      t += step;
    }
  };
  const blank: Row = [];
  const chapter = (label: string) => chapters.push({ label, at: t, end: t });
  const closeChapter = () => {
    chapters[chapters.length - 1].end = t;
  };

  chapter('add a marketplace');
  prompt('uze market add hiukky/ai');
  spin('Adding hiukky/ai...', 1100);
  rows([
    report('market add'),
    blank,
    [{ text: '+ ', tone: 'success' }, { text: 'hiukky/ai', tone: 'ink' }, { text: '   cloned', tone: 'muted' }],
    blank,
    [{ text: '1 marketplace added', tone: 'ink' }, { text: ' [1.1s]', tone: 'muted' }],
    blank,
  ]);
  t += 800;
  closeChapter();

  chapter('add a plugin');
  prompt('uze git@ai');
  spin('Installing git@ai...', 900);
  rows([report('install'), blank, [{ text: '+ ', tone: 'success' }, { text: 'git@ai', tone: 'ink' }, { text: '   3f2a91c', tone: 'muted' }], blank]);
  events.push({ kind: 'lanes', at: t, lanes: delivered, laneStagger: 30, rowStagger: 12 });
  t += 30 * 3 + 12 * 5 + 1400;
  rows([blank, [{ text: '1 plugin added to this project', tone: 'ink' }, { text: ' [652ms]', tone: 'muted' }], blank]);
  t += 1000;
  closeChapter();

  chapter('the project file');
  prompt('cat agents.yaml');
  rows(manifest);
  rows([blank]);
  t += 2000;
  closeChapter();

  chapter('open the workspace');
  prompt('uze workspace');
  t += 300;
  events.push({ kind: 'screen', at: t });
  t += WORKSPACE_LENGTH;
  closeChapter();

  return { events, chapters, end: t };
}

const script = build();
const screenAt = script.events.find((event) => event.kind === 'screen')?.at ?? Infinity;

function toneClass(tone: Tone | undefined) {
  switch (tone) {
    case 'success':
      return 'text-success';
    case 'warn':
      return 'text-warn';
    case 'title':
      return 'font-semibold text-ink';
    case 'ink':
      return 'text-ink';
    default:
      return 'text-muted';
  }
}

// Lines wrap the way a narrow terminal wraps them, never scroll sideways.
function Line({ row }: { row: Row }) {
  if (row.length === 0) return <div aria-hidden>&nbsp;</div>;
  return (
    <div className="whitespace-pre-wrap break-words">
      {row.map((span, index) => (
        <span key={index} className={toneClass(span.tone)}>
          {span.text}
        </span>
      ))}
    </div>
  );
}

function Prompt({ children, caret }: { children?: ReactNode; caret: boolean }) {
  return (
    <div className="whitespace-pre">
      <span className="text-muted">❯ </span>
      <span className="text-ink">{children}</span>
      {caret ? <span className="session-caret" aria-hidden /> : null}
    </div>
  );
}

// The agent's name as a filled chip: the same mark stands at the head of a
// lane here and wherever else the page names an agent.
export function AgentChip({ children }: { children: ReactNode }) {
  return (
    <span
      className="inline-block px-1.5 py-px font-mono text-[0.92em] font-semibold leading-snug"
      style={{ background: 'var(--color-ink)', color: 'var(--color-paper)' }}
    >
      {children}
    </span>
  );
}

function Lanes({ event, t }: { event: Extract<Event, { kind: 'lanes' }>; t: number }) {
  return (
    <div className="my-2 grid grid-cols-2 gap-x-6 gap-y-5 sm:grid-cols-4 sm:gap-x-4">
      {event.lanes.map((lane, laneIndex) => {
        const laneAt = event.at + laneIndex * event.laneStagger;
        if (t < laneAt) return <div key={lane.name} />;
        return (
          <div key={lane.name} className="min-w-0 border-l border-line pl-3">
            <AgentChip>{lane.name}</AgentChip>
            <div className="mt-2 space-y-0.5">
              {lane.rows.map((row, rowIndex) =>
                t >= laneAt + (rowIndex + 1) * event.rowStagger ? <Line key={rowIndex} row={row} /> : null,
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function Scrollback({ t }: { t: number }) {
  const out: ReactNode[] = [];
  let caretShown = false;
  for (let i = script.events.length - 1; i >= 0; i--) {
    const event = script.events[i];
    if (t < event.at) continue;
    let node: ReactNode = null;
    switch (event.kind) {
      case 'prompt': {
        const typed = Math.min(event.text.length, Math.floor((t - event.at) / event.charMs));
        const typing = t < event.done;
        node = (
          <Prompt key={i} caret={typing && !caretShown}>
            {event.text.slice(0, typed)}
          </Prompt>
        );
        if (typing) caretShown = true;
        break;
      }
      case 'row':
        node = <Line key={i} row={event.row} />;
        break;
      case 'spin':
        if (t < event.until) {
          const frame = SPINNER[Math.floor((t - event.at) / 80) % SPINNER.length];
          node = (
            <div key={i} className="whitespace-pre">
              <span className="text-accent">{frame} </span>
              <span className="text-muted">{event.text}</span>
            </div>
          );
        }
        break;
      case 'lanes':
        node = <Lanes key={i} event={event} t={t} />;
        break;
      case 'screen':
        break;
    }
    if (node) out.push(node);
  }
  out.reverse();
  return <>{out}</>;
}

const HOLD_MS = 6000;

export function ConsoleSession() {
  const [t, setT] = useState(0);
  const [still, setStill] = useState(false);
  const stream = useRef<HTMLDivElement>(null);
  const clock = useRef(0);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);

  // One clock for the whole session, advanced a fixed step per tick rather
  // than read from the wall: text appearing at 30 frames a second is more
  // than the eye asks of it, and a stepped clock plays the same session in
  // every browser, a throttled tab and a headless one included. Jumping
  // moves the clock, so a chapter picked mid-play starts from its first
  // keystroke.
  const stop = () => {
    if (timer.current) clearInterval(timer.current);
    timer.current = null;
  };
  // It loops: the workspace is held long enough to be read, then the
  // session starts over, so a reader who arrives late still sees it begin.
  const play = (from: number) => {
    stop();
    clock.current = from;
    setT(from);
    timer.current = setInterval(() => {
      clock.current += TICK_MS;
      if (clock.current >= script.end + HOLD_MS) clock.current = 0;
      setT(Math.min(clock.current, script.end));
    }, TICK_MS);
  };

  useEffect(() => {
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (reduced) {
      setStill(true);
      setT(script.end);
      return;
    }
    play(0);
    return stop;
    // Runs once: the script is a module constant.
  }, []);

  useLayoutEffect(() => {
    const el = stream.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [t]);

  const current = script.chapters.reduce((found, chapter, index) => (t >= chapter.at ? index : found), 0);
  const onScreen = t >= screenAt;

  return (
    <div className="border-y border-line">
      {/* The chapters, where a console keeps its tabs: the one playing is
          filled, and any of them can be picked. */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-line py-2 font-mono text-xs sm:justify-between">
        {script.chapters.map((chapter, index) => {
          const active = index === current;
          return (
            <button
              key={chapter.label}
              type="button"
              onClick={() => (still ? setT(chapter.end) : play(chapter.at))}
              aria-current={active ? 'step' : undefined}
              className={`inline-flex items-center gap-2 py-1 transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
                active ? 'text-ink' : 'text-muted hover:text-ink'
              }`}
            >
              <span
                className="inline-flex size-5 items-center justify-center text-[11px] font-semibold"
                style={
                  active
                    ? { background: 'var(--color-accent)', color: 'var(--color-paper)' }
                    : { boxShadow: 'inset 0 0 0 1px var(--color-line)' }
                }
              >
                {index + 1}
              </span>
              {chapter.label}
            </button>
          );
        })}
      </div>

      <div className="h-[30rem] font-mono sm:h-[31rem]" aria-hidden>
        {onScreen ? (
          <Workspace t={t - screenAt} />
        ) : (
          <div
            ref={stream}
            className="session-scrollback h-full overflow-hidden py-4 text-[12.5px] leading-[1.6] sm:text-[13.5px]"
          >
            <Scrollback t={t} />
          </div>
        )}
      </div>
      <p className="sr-only">
        A terminal session: uze market add hiukky/ai adds a marketplace; uze git@ai installs the git
        plugin and Claude Code, Codex, OpenCode and Antigravity each report the skills, agent and hook
        they received; agents.yaml records the plugin and the project&apos;s worktree policy; then uze
        workspace opens with three agents, each on a branch in its own worktree, then shows the change the
        first one worked from, the project&apos;s architecture diagram and the diff it left.
      </p>
    </div>
  );
}
