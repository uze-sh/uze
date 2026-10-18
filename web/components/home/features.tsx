import type { ReactNode } from 'react';
import Link from 'next/link';
import { ArrowRight, Check } from 'lucide-react';
import { HarnessMark, harnessMarks } from '@/components/harness-marks';

// What uze does, one claim per row: the picture on one side and the words on
// the other, swapping sides so the eye zigzags down instead of reading a
// column of cards. Every picture is a frame of uze itself (its CLI's output,
// its sidebar, its menus) redrawn in HTML, so it stays sharp, themed and
// searchable; and every claim is one the docs it links to back up.
export function Features() {
  return (
    <div className="space-y-24 sm:space-y-32">
      <Feature
        eyebrow="Plugins"
        title="One plugin. Every agent."
        body="Install a plugin once and each agent receives it through its own mechanism: a real plugin where the agent has one, its native files where it does not."
        points={[
          'Skills, subagents, MCP servers and hooks, in each agent’s own format',
          'Recorded in agents.yaml, so a teammate’s uze install gets the same',
          'A receipt for every file placed: drift stops uze, it never overwrites',
        ]}
        more={{ href: '/docs/plugins/delivery', label: 'How delivery works' }}
        visual={<InstallVisual />}
      />
      <Feature
        eyebrow="Workspace"
        title="Every agent on a branch of its own."
        body="Start four agents on one repository and none of them touches another’s files, or yours. The sidebar says where each one stands, read from Git, not from what the agent claims."
        points={[
          'A worktree per agent, reused warm: no fresh node_modules, no cold build',
          'Branches named from the work, like fix/login-timeout',
          'Close an agent mid-task and its work is kept, never lost',
        ]}
        more={{ href: '/docs/workspace/agents', label: 'Where an agent works' }}
        visual={<SidebarVisual />}
        flip
      />
      <Feature
        eyebrow="Delivery"
        title="One key brings the work home."
        body="When an agent is ready, Alt+I rebases its branch onto yours, runs your checks, and merges it or pushes it. Nothing reaches your branch until the checks pass."
        points={[
          'A conflict or a failed check goes back to the agent, in your project’s words',
          'Your checks run only after you read and approve them',
          'Deliver all: every ready agent in one go',
        ]}
        more={{ href: '/docs/workspace/agents#bringing-the-work-home', label: 'Bringing the work home' }}
        visual={<DeliveryVisual />}
      />
      <Feature
        eyebrow="Continuity"
        title="Out of tokens? Switch harness, keep going."
        body="When one harness hits its limit, the same agent carries on in another: same directory, same branch, same name. The one it left keeps its conversation for when you switch back."
        points={[
          'A briefing of what it was asked, said and committed',
          'Its uncommitted files and last command included',
          'Kept out of your checkout, readable by you alone',
        ]}
        more={{ href: '/docs/workspace/agents#continuing-in-another-harness', label: 'Continuing in another harness' }}
        visual={<ContinueVisual />}
        flip
      />
    </div>
  );
}

function Feature({
  eyebrow,
  title,
  body,
  points,
  more,
  visual,
  flip = false,
}: {
  eyebrow: string;
  title: string;
  body: string;
  points: string[];
  more: { href: string; label: string };
  visual: ReactNode;
  flip?: boolean;
}) {
  return (
    <article className="grid items-center gap-x-16 gap-y-10 lg:grid-cols-2">
      {/* The words come first in the document and when stacked: the picture
          proves a claim, and a reader has to have met the claim first. */}
      <div className="min-w-0">
        <p className="mb-4 font-mono text-[13px] font-semibold tracking-tight text-success">{eyebrow}</p>
        <h2 className="text-balance text-[2rem] font-semibold leading-[1.05] tracking-[-0.035em] text-ink sm:text-[2.5rem]">
          {title}
        </h2>
        <p className="mt-5 max-w-[34rem] text-pretty text-[16px] leading-relaxed text-muted sm:text-[17px]">{body}</p>
        <ul className="mt-6 space-y-2.5">
          {points.map((point) => (
            <li key={point} className="flex gap-3 text-[15px] leading-snug text-ink">
              <Check className="mt-0.5 size-4 shrink-0 text-success" strokeWidth={2.25} aria-hidden />
              {point}
            </li>
          ))}
        </ul>
        <Link
          href={more.href}
          className="group mt-7 inline-flex items-center gap-1.5 font-mono text-[13px] font-semibold text-ink transition-colors hover:text-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        >
          {more.label}
          <ArrowRight className="size-3.5 transition-transform group-hover:translate-x-0.5" aria-hidden />
        </Link>
      </div>
      <div className={`min-w-0 ${flip ? '' : 'lg:order-first'}`}>{visual}</div>
    </article>
  );
}

// Always the dark palette, for the reason the recording is: uze's terminal
// has no light theme, and a frame of it drawn light shows a uze that does
// not exist.
function Frame({ title, children }: { title: string; children: ReactNode }) {
  return (
    <figure
      className="dark overflow-hidden rounded-xl border border-line bg-paper font-mono text-[12.5px] leading-[1.7] text-ink shadow-[0_24px_60px_-24px_rgb(0_0_0/0.45)]"
      aria-label={title}
    >
      <div className="flex items-center gap-1.5 border-b border-line px-4 py-2.5">
        {['#ff5f57', '#febc2e', '#28c840'].map((dot) => (
          <span key={dot} className="size-2.5 rounded-full" style={{ background: dot }} aria-hidden />
        ))}
        <span className="ms-3 truncate text-[11.5px] text-muted">{title}</span>
      </div>
      <div className="overflow-x-auto px-5 py-4">{children}</div>
    </figure>
  );
}

const Prompt = ({ children }: { children: ReactNode }) => (
  <p>
    <span className="text-muted select-none">my-app $ </span>
    {children}
  </p>
);

function InstallVisual() {
  const columns = harnessMarks.map((harness) => harness.name);
  const rows: [string, string[]][] = [
    ['skill changelog', ['✓', '✓', '✓', '✓']],
    ['skill review', ['✓', '✓', '✓', '✓']],
    ['delivery', ['package', 'package', 'per item', 'package']],
  ];
  return (
    <Frame title="~/my-app">
      <Prompt>uze install review@acme</Prompt>
      <p className="mt-2">
        <span className="text-success">+</span> review@acme <span className="ms-3 text-muted">1648a4e</span>
      </p>
      <p className="text-muted">1 plugin added to this project [911ms]</p>
      <div className="mt-4">
        <Prompt>uze inspect review</Prompt>
      </div>
      <table className="mt-3 w-full min-w-[30rem] border-separate border-spacing-x-0 text-left">
        <thead>
          <tr>
            <th className="w-[9.5rem]" />
            {columns.map((name, index) => (
              <th key={name} className="pb-1.5 font-normal" title={name}>
                <span className="inline-flex items-center gap-1.5 text-muted">
                  <HarnessMark icon={harnessMarks[index].icon} className="size-3.5 shrink-0" />
                  <span className="hidden sm:inline">{name.replace(' Code', '')}</span>
                </span>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map(([label, cells]) => (
            <tr key={label}>
              <td className="text-muted">{label}</td>
              {cells.map((cell, index) => (
                <td key={columns[index]} className={cell === '✓' ? 'text-success' : 'text-ink'}>
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      <p className="mt-3">
        <span className="text-success">✓</span> 5 files in place, none drifted
      </p>
    </Frame>
  );
}

const STATES: { work: string; harness: number; state: string; tone: string; detail?: string }[] = [
  { work: 'fix/login-timeout', harness: 0, state: 'ready', tone: 'text-success', detail: '→ main ↑3' },
  { work: 'feat/rate-limit', harness: 1, state: 'uncommitted', tone: 'text-warn' },
  { work: 'docs/api-keys', harness: 2, state: 'in review', tone: 'text-info', detail: '#41' },
  { work: 'refactor/db-pool', harness: 3, state: 'unfinished', tone: 'text-muted' },
];

function SidebarVisual() {
  return (
    <Frame title="~/demo-api · uze">
      <p className="text-[11px] tracking-wide text-muted">work</p>
      <p className="mt-2 font-semibold">
        <span className="me-2 text-muted">⌄</span>demo-api
      </p>
      <ul className="mt-1.5 space-y-1">
        {STATES.map(({ work, harness, state, tone, detail }, index) => (
          <li
            key={work}
            className={`flex items-center gap-3 border-s-2 py-1.5 ps-3 pe-2 ${
              index === 0 ? 'border-ink bg-surface' : 'border-transparent'
            }`}
          >
            <HarnessMark icon={harnessMarks[harness].icon} className="size-3.5 shrink-0" />
            <span className="min-w-0 flex-1 truncate">{work}</span>
            <span className={`shrink-0 ${tone}`}>{state}</span>
            {detail ? <span className="w-[5.5rem] shrink-0 text-end text-muted">{detail}</span> : <span className="w-[5.5rem] shrink-0" />}
          </li>
        ))}
      </ul>
      <p className="mt-4 border-t border-line pt-3 text-[11.5px] text-muted">
        .worktrees/ <span className="text-ink">4 checkouts</span>, one per agent
      </p>
    </Frame>
  );
}

function DeliveryVisual() {
  const steps = [
    ['rebased onto origin/main', '3 commits'],
    ['gate  npm test', 'passed'],
    ['pushed fix/login-timeout', 'origin'],
  ];
  return (
    <Frame title="fix/login-timeout · delivering">
      <div className="flex items-center justify-between gap-4">
        <p className="font-semibold">fix/login-timeout</p>
        <span className="rounded-sm border border-success/50 px-2 text-[11px] text-success">ready</span>
      </div>
      <p className="text-muted">→ main ↑3 · +142 −38</p>
      <ol className="mt-4 space-y-1.5 border-s border-line ps-4">
        {steps.map(([step, note]) => (
          <li key={step} className="flex items-center gap-3">
            <span className="text-success">✓</span>
            <span className="flex-1">{step}</span>
            <span className="text-muted">{note}</span>
          </li>
        ))}
      </ol>
      <div className="mt-5 flex items-center justify-between gap-4 border-t border-line pt-4">
        <span className="text-success">delivered</span>
        <span className="inline-flex items-center gap-2 text-muted">
          deliver <kbd className="rounded-sm border border-line bg-surface px-1.5 text-[11px] text-ink">Alt+I</kbd>
        </span>
      </div>
    </Frame>
  );
}

function ContinueVisual() {
  const [claude, ...others] = harnessMarks;
  const briefing = [
    ['asked', 'add rate limiting to /api/login'],
    ['said last', 'middleware done, tests next'],
    ['ran last', 'npm test -- rate-limit'],
    ['work', '2 commits, 3 uncommitted files'],
  ];
  return (
    <Frame title="feat/rate-limit · Claude Code">
      <p className="flex items-center gap-2">
        <HarnessMark icon={claude.icon} className="size-3.5 shrink-0" />
        <span className="text-warn">usage limit reached</span>
      </p>
      <div className="mt-3 grid gap-4 sm:grid-cols-[11rem_minmax(0,1fr)]">
        <ul className="self-start rounded-md border border-line bg-surface py-1.5">
          <li className="px-3 pb-1 text-[11px] text-muted">Continue in…</li>
          {others.map((harness, index) => (
            <li
              key={harness.name}
              className={`flex items-center gap-2 px-3 py-0.5 ${index === 0 ? 'bg-ink text-paper' : ''}`}
            >
              <HarnessMark icon={harness.icon} className="size-3.5 shrink-0" />
              {harness.name}
            </li>
          ))}
        </ul>
        <div className="min-w-0">
          <p className="text-[11px] text-muted">briefing for Codex</p>
          <dl className="mt-1.5 space-y-1">
            {briefing.map(([term, value]) => (
              <div key={term} className="grid grid-cols-[5.5rem_minmax(0,1fr)] gap-2">
                <dt className="text-muted">{term}</dt>
                <dd className="truncate">{value}</dd>
              </div>
            ))}
          </dl>
        </div>
      </div>
    </Frame>
  );
}
