import type { ReactNode } from 'react';
import Link from 'next/link';
import { ArrowRight, Check } from 'lucide-react';
import recorded from '@/lib/home-takes.json';
import { Takes, type Run, type Style } from '@/components/home/takes';

// What uze does, one claim per row: the picture on one side and the words on
// the other, swapping sides so the eye zigzags down instead of reading a
// column of cards. Every picture is a take: a screen of the real CLI or
// workspace, captured cell by cell from a build of main in a disposable
// world (`lib/home-takes.json`). Two edits only: the stand-in harness's own
// text is cleared from its pane, and the strip's last cell, which the capture
// lost the ground of, is given it back. Every claim is one the docs it links
// to back up.
export function Features() {
  return (
    <>
      {/* The rows answer one question, so it is asked once above them, and
          the heading is the link a reader shares to land on the answer. */}
      <header className="mx-auto mb-20 max-w-[40rem] text-center sm:mb-28">
        <h2 className="text-balance text-[2.25rem] font-semibold leading-[1.02] tracking-[-0.04em] text-ink sm:text-[3rem]">
          {/* The mark sits outside the line, so the hidden one does not
              push a centred heading off its centre. */}
          <a href="#features" className="group relative">
            What uze does for each of your agents
            <span
              className="absolute bottom-0 left-full ms-3 text-muted opacity-0 transition-opacity group-hover:opacity-100"
              aria-hidden
            >
              #
            </span>
          </a>
        </h2>
        <p className="mt-5 text-pretty text-[16px] leading-relaxed text-muted sm:text-[17px]">
          Install once. Run them side by side. Bring the work home.
        </p>
      </header>
      <div className="space-y-28 sm:space-y-44">
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
          visual={<Moments title="~/demo-app" takes={[['install', 'install'], ['inspect', 'inspect']]} />}
        />
        <Feature
          eyebrow="Workspace"
          title="Every agent on a branch of its own."
          body="Start four agents on one repository and none of them touches another’s files, or yours. The sidebar says where each one stands, read from Git, not from what the agent claims."
          points={[
            'A worktree per agent, reused warm: no fresh node_modules, no cold build',
            'Branches named from the work: add rate limiting is feat/add-rate-limiting',
            'Close an agent mid-task and its work is kept, never lost',
          ]}
          more={{ href: '/docs/workspace/agents', label: 'Where an agent works' }}
          visual={
            <Moments
              title="uze workspace"
              takes={[
                ['new', 'new agent'],
                ['agents', 'agents'],
                ['recent', 'recent'],
              ]}
            />
          }
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
          visual={
            <Moments
              title="uze workspace"
              takes={[
                ['ready', 'ready'],
                ['delivering', 'delivering'],
                ['delivered', 'delivered'],
              ]}
            />
          }
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
          visual={<Moments title="uze workspace" takes={[['actions', 'alt+.'], ['continued', 'continued']]} />}
          flip
        />
      </div>
    </>
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
        <h3 className="text-balance text-[2rem] font-semibold leading-[1.05] tracking-[-0.035em] text-ink sm:text-[2.5rem]">
          {title}
        </h3>
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

type Recorded = keyof typeof recorded.takes;

function Moments({ title, takes }: { title: string; takes: [Recorded, string][] }) {
  return (
    <Takes
      title={title}
      cols={recorded.cols}
      styles={recorded.styles as Style[]}
      takes={takes.map(([name, label]) => ({ label, rows: recorded.takes[name] as Run[][] }))}
    />
  );
}
