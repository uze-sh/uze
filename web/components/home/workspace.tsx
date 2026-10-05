import type { ReactNode } from 'react';
import { ExtensionSurface, type Surface } from '@/components/home/extensions';

// The workspace as `uze workspace` draws it, after the recorded frame in
// web/public/uze-demo-poster.png: spaces down the side, each holding its
// agents as the work's name over the harness running it; the checkout's
// history under them; the selected agent's tab and the extensions over the
// main area; the agent's own session in the pane, in a checkout of its own;
// and the keys that act here on the last line. Once the first agent is done,
// the selection moves to it and spec, arch and code open in turn over the
// main area. Nothing is on it the product does not do, and every key is one
// docs/workspace/keys.mdx lists.

type Agent = {
  name: string;
  harness: string;
  branch: string;
  checkout: string;
  prompt: string;
};

const AGENTS: Agent[] = [
  {
    name: 'routes',
    harness: 'claude',
    branch: 'feat/routes',
    checkout: '.worktrees/7qk2x1',
    prompt: 'return 404 for routes nobody declared',
  },
  {
    name: 'health',
    harness: 'codex',
    branch: 'test/health',
    checkout: '.worktrees/m3zp0c',
    prompt: 'cover GET /health with a test',
  },
  {
    name: 'readme',
    harness: 'opencode',
    branch: 'docs/readme',
    checkout: '.worktrees/bczzk0',
    prompt: 'add a Routes section to README.md listing every route in src/router.rs',
  },
];
const SELECTED = 2;

const TIMELINE: [string, string][] = [
  ['docs(openspec): plan rate limiting', '27h'],
  ['chore: declare the project in agents.yaml', '2d'],
  ['docs: add AGENTS.md', '3d'],
  ['feat(config): read settings', '4d'],
  ['feat(api): add the health route', '7d'],
  ['chore: initial commit', '9d'],
];

const ACTIVITY = ['Thought for 1.2s', 'Read src/router.rs', 'Writing README.md'];

// When each thing appears, in milliseconds after the screen opens.
const PLACED = [250, 550, 850];
const PROMPT_AT = 1000;
const ACTIVITY_AT = [1300, 1700, 2100];
const READY_AT = 2700;
// Each extension is opened on the finished agent, takes its one gesture
// halfway through, and holds long enough to be read.
const EXTENSIONS: [Surface, number][] = [
  ['spec', 3600],
  ['arch', 7000],
  ['code', 10400],
];
const EXTENSION_MS = 3400;
export const WORKSPACE_LENGTH = EXTENSIONS[2][1] + EXTENSION_MS;

function Key({ k, children }: { k: string; children: ReactNode }) {
  return (
    <span className="whitespace-nowrap text-muted">
      <span className="font-semibold text-ink">{k}</span> {children}
    </span>
  );
}

function AgentEntry({ agent, selected, ready }: { agent: Agent; selected: boolean; ready: boolean }) {
  return (
    <div
      className={`flex items-start gap-2 px-2 py-1 ${selected ? 'bg-surface' : ''}`}
      style={selected ? { boxShadow: 'inset 2px 0 0 var(--color-accent)' } : undefined}
    >
      <span className={`w-3 shrink-0 text-center ${ready ? 'text-success' : 'text-muted'}`}>{ready ? '✓' : '⠿'}</span>
      <span className="min-w-0 flex-1">
        <span className={`block truncate ${selected ? 'font-semibold text-ink' : 'text-ink'}`}>{agent.name}</span>
        <span className={`block truncate ${selected ? 'text-warn' : 'text-muted'}`}>{agent.harness}</span>
      </span>
      {ready ? <span className="shrink-0 text-accent">±</span> : null}
    </div>
  );
}

export function Workspace({ t }: { t: number }) {
  const placed = PLACED.filter((at) => t >= at).length;
  const ready = t >= READY_AT;
  const shown = ACTIVITY.filter((_, i) => t >= ACTIVITY_AT[i]).length;
  const open = EXTENSIONS.findLast(([, at]) => t >= at);
  const surface = open?.[0];
  const step = open ? t >= open[1] + EXTENSION_MS * 0.45 : false;
  const selected = AGENTS[surface ? 0 : SELECTED];
  const pane = placed > SELECTED;

  return (
    <div className="flex h-full min-h-0 flex-col text-[12px] leading-[1.5] sm:text-[12.5px]">
      {/* The bar: the sidebar's own head, then the selected agent's tab, a
          new one, and the extensions. */}
      <div className="flex h-9 shrink-0 items-center border-b border-line">
        <div className="flex h-full w-32 shrink-0 items-center gap-2 border-r border-line px-3 sm:w-56">
          <span className="font-semibold text-muted">work</span>
          <span className="ml-auto font-semibold text-accent">+ space</span>
          <span className="text-muted max-sm:hidden">≡</span>
        </div>
        <div className="flex min-w-0 flex-1 items-center gap-2 px-3">
          {pane ? (
            <span className="flex items-center gap-1.5 bg-surface px-2 py-0.5 font-semibold text-ink">
              <span className="text-accent">✦</span>
              {selected.name}
            </span>
          ) : null}
          <span className="text-muted">/</span>
          <span className="bg-surface px-2 py-0.5 text-muted">+</span>
          <span className="ml-auto flex items-center gap-1.5 max-sm:hidden">
            {ready ? (
              <span className="mr-2">
                <span className="text-success">+31</span> <span className="text-danger">−4</span>
              </span>
            ) : null}
            {(['spec', 'arch', 'code'] as Surface[]).map((name) => (
              <span
                key={name}
                className={`px-2 py-0.5 ${name === surface ? 'font-semibold' : 'bg-surface text-muted'}`}
                style={
                  name === surface
                    ? {
                        background: 'var(--color-ink)',
                        color: 'var(--color-paper)',
                      }
                    : undefined
                }
              >
                {name}
              </span>
            ))}
            <span className="ml-1 text-accent">✦</span>
          </span>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 max-sm:flex-col">
        {/* Spaces, each holding its agents; the checkout's history under them. */}
        <aside className="flex w-32 shrink-0 flex-col border-r border-line py-2 max-sm:w-auto max-sm:border-r-0 max-sm:border-b sm:w-56">
          <div className="flex items-center gap-2 bg-surface px-2 py-1">
            <span className="text-muted">▾</span>
            <span className="font-semibold text-ink">api</span>
            <span className="ml-auto font-semibold text-warn">new</span>
          </div>
          <div className="max-sm:grid max-sm:grid-cols-3 max-sm:gap-1 max-sm:px-1 max-sm:py-1 sm:mt-1 sm:pl-3">
            {AGENTS.slice(0, placed).map((agent, i) => (
              <AgentEntry key={agent.name} agent={agent} selected={agent === selected} ready={ready && i === 0} />
            ))}
          </div>
          <div className="mt-auto max-sm:hidden">
            {/* The selected agent's change, once it is the one with a plan. */}
            {surface ? (
              <div className="mb-1">
                <div className="flex items-center gap-2 bg-surface px-2 py-1">
                  <span className="text-muted">▾</span>
                  <span className="font-semibold text-ink">tasks</span>
                  <span className="ml-auto text-muted">2/4</span>
                </div>
                <div className="flex items-center gap-2 px-2 py-0.5 pl-6 text-ink">
                  <span className="truncate">add-not-found</span>
                  <span className="ml-auto text-muted">2/4</span>
                </div>
              </div>
            ) : null}
            <div className="flex items-center gap-2 bg-surface px-2 py-1">
              <span className="text-muted">▾</span>
              <span className="font-semibold text-ink">timeline</span>
              <span className="ml-auto truncate text-muted">{pane ? selected.branch : 'main'}</span>
            </div>
            <div className="mt-1 space-y-0.5 px-2">
              {TIMELINE.map(([subject, age]) => (
                <div key={subject} className="flex items-center gap-2 text-muted">
                  <span className="size-1.5 shrink-0 rounded-full bg-warn" />
                  <span className="truncate">{subject}</span>
                  <span className="ml-auto shrink-0">{age}</span>
                </div>
              ))}
            </div>
          </div>
        </aside>

        {/* The agent's own harness, in its own checkout. */}
        {surface ? (
          <ExtensionSurface surface={surface} step={step} />
        ) : (
          <section className="flex min-h-0 min-w-0 flex-1 flex-col p-3">
            {pane && t >= PROMPT_AT ? (
              <>
                <div className="border-l-2 border-accent bg-surface px-3 py-2 text-ink">{selected.prompt}</div>
                <div className="mt-3 space-y-1.5 px-1 text-muted">
                  {ACTIVITY.slice(0, shown).map((line, i) => (
                    <div key={line}>
                      <span className={i === shown - 1 ? 'text-accent' : 'text-muted'}>
                        {i === shown - 1 ? '●' : '✓'}
                      </span>{' '}
                      {line}
                    </div>
                  ))}
                </div>
              </>
            ) : null}
            <div className="mt-auto">
              <div className="border-l-2 border-accent bg-surface px-3 py-2">
                <span className="session-caret" aria-hidden />
                <div className="mt-1 text-muted">
                  <span className="text-accent">Build</span>
                </div>
              </div>
              <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px]">
                <Key k="esc">interrupt</Key>
                <span className="ml-auto flex gap-4">
                  <Key k="shift+tab">agents</Key>
                  <Key k="ctrl+p">commands</Key>
                </span>
              </div>
              <div className="mt-2 truncate text-right text-[11px] text-muted">~/api/{selected.checkout}</div>
            </div>
          </section>
        )}
      </div>

      {/* The keys that act here, the way the workspace says them. An open
          extension says its own, under it. */}
      <div
        hidden={Boolean(surface)}
        className="flex min-h-8 shrink-0 flex-wrap items-center gap-x-4 gap-y-0.5 border-t border-line px-3 py-1 text-[11px]"
      >
        <Key k="alt+n">new agent</Key>
        <Key k="alt+i">deliver</Key>
        <Key k="ctrl+o">manage</Key>
        <Key k="F1">keys</Key>
      </div>
    </div>
  );
}
