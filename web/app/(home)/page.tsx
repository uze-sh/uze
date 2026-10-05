import Link from 'next/link';
import { Check, Plus } from 'lucide-react';
import { InstallTabs } from '@/components/install-tabs';
import { HarnessMark, harnessMarks, homepageOf } from '@/components/harness-marks';
import { AroundIllustration } from '@/components/around-illustration';
import { faq, pillars } from '@/lib/home-content';
import { TrademarkNotice } from '@/components/trademark-notice';
import matrix from '@/lib/harness-matrix.json';
import { UzeMark } from '@/components/uze-mark';
import { HeroIllustration } from '@/components/hero-illustration';
import { WorkspaceIllustration } from '@/components/workspace-illustration';

type Capability = 'context' | 'skills' | 'mcp' | 'agents' | 'hooks' | 'session' | 'package';

const columns: [Capability, string][] = [
  ['context', 'AGENTS.md'],
  ['skills', 'Skills'],
  ['mcp', 'MCP'],
  ['agents', 'Subagents'],
  ['hooks', 'Hooks'],
  ['session', 'Session'],
  ['package', 'Plugin'],
];

// The question a reader is actually asking of this table is "does it work
// here", so that is what the mark answers: every delivered route gets the
// same check, and the route word beside it is the detail. Dimming `bridge`
// and `adapted` against `native` answered a different question and made two
// working routes look broken.
function Route({ value }: { value: string }) {
  if (value === 'none') {
    return (
      <span className="font-mono text-xs text-muted/60" title="not delivered through this route">
        —
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 font-mono text-xs whitespace-nowrap text-ink">
      <Check className="size-3.5 shrink-0 text-accent" aria-hidden strokeWidth={3} />
      {value}
    </span>
  );
}

// What the section below it pictures, said before it is shown: each
// illustration is one module, and neither reads as that on its own.
function SectionHeading({ eyebrow, title, body }: { eyebrow: string; title: string; body: string }) {
  return (
    <header className="text-center">
      <p className="font-mono text-xs text-accent">{eyebrow}</p>
      <h2 className="mt-3 font-mono text-2xl font-bold tracking-tight text-ink sm:text-3xl">{title}</h2>
      <p className="mx-auto mt-3 max-w-[56ch] text-sm leading-relaxed text-muted sm:text-base">{body}</p>
    </header>
  );
}

export default function HomePage() {
  return (
    <main className="flex flex-col items-center flex-1 px-6 font-sans">
      {/* Hero. Holds the first screen on its own — the banner and the h-14
          header are the only chrome above it — so the illustrations below are
          something you arrive at by scrolling, not something competing with
          the headline for the same view. */}
      <section className="flex w-full max-w-5xl flex-col justify-center min-h-[calc(100dvh_-_var(--uze-banner-height)_-_3.5rem)] py-14 text-center">
        {/* Said first, because it is the first thing a reader gets wrong: a
            tool "for coding agents" reads as one more coding agent. */}
        <p className="mx-auto inline-flex items-center gap-2 border border-line px-3 py-1 font-mono text-[11px] text-muted sm:text-xs">
          <span className="text-accent">●</span>
          Not an agent. No model, no API key.
        </p>
        <h1 className="mx-auto mt-7 font-mono font-bold tracking-tight text-ink text-[2.25rem] leading-[1.06] sm:text-5xl lg:text-[3.5rem]">
          The package manager
          <br />
          <span className="text-accent">for coding agents.</span>
        </h1>
        <p className="mx-auto mt-6 max-w-[38rem] text-pretty text-lg leading-relaxed text-muted">
          Install skills, MCP servers, hooks and{' '}
          <code className="font-mono text-ink">AGENTS.md</code> into Claude Code, Codex, OpenCode
          and Antigravity, then run them side by side, each in its own worktree.
        </p>

        <div className="mx-auto mt-9 flex w-full max-w-xl flex-col items-stretch gap-3 sm:flex-row sm:items-start">
          <div className="min-w-0 flex-1">
            <InstallTabs />
          </div>
          <Link
            href="/docs/quickstart"
            className="inline-flex shrink-0 items-center justify-center border border-ink bg-ink px-5 py-2.5 font-mono text-[13px] text-paper transition-opacity hover:opacity-85 sm:mt-[30px]"
          >
            Get started
          </Link>
        </div>

        {/* Labelled "works with", never "powered by": the marks are the
            agents uze serves, not anything uze is made of. */}
        <div className="mt-12">
          <p className="font-mono text-[11px] text-muted">Works with</p>
          <ul className="mt-4 flex flex-wrap items-center justify-center gap-x-8 gap-y-4">
            {harnessMarks.map((harness) => (
              <li key={harness.name}>
                <a
                  href={homepageOf(harness.name)}
                  target="_blank"
                  rel="noreferrer noopener"
                  className="flex items-center gap-2 text-ink transition-colors hover:text-accent"
                >
                  <HarnessMark icon={harness.icon} className="size-5" />
                  <span className="font-mono text-sm font-semibold">{harness.name}</span>
                </a>
              </li>
            ))}
          </ul>
        </div>
      </section>

      {/* Where uze sits, before any picture of a terminal with an agent in
          it: that picture is what reads as "another agent" when it comes
          first. */}
      <section className="w-full max-w-5xl border-t border-line py-20 sm:py-24">
        <SectionHeading
          eyebrow="Where uze sits"
          title="Around your agents, not instead of them."
          body="uze gives the agents you use the same plugins and a worktree each, then brings their work back to your repo."
        />
        <div className="mt-12">
          <AroundIllustration />
        </div>
        <div className="mx-auto mt-14 grid max-w-4xl gap-px border border-line bg-line sm:grid-cols-2">
          <div className="bg-paper p-6">
            <h3 className="font-mono text-xs text-accent">uze is</h3>
            <ul className="mt-4 space-y-3 text-sm leading-relaxed text-ink">
              <li>A package manager for what your agents share: skills, MCP servers, hooks, subagents, AGENTS.md.</li>
              <li>A terminal that runs several agents side by side, each in its own git worktree.</li>
              <li>One Rust binary, open source under Apache 2.0.</li>
            </ul>
          </div>
          <div className="bg-paper p-6">
            <h3 className="font-mono text-xs text-muted">uze is not</h3>
            <ul className="mt-4 space-y-3 text-sm leading-relaxed text-muted">
              <li>A coding agent. Your agents do the coding.</li>
              <li>An orchestrator. You talk to each agent exactly as before.</li>
              <li>A new plugin format. It installs the open standards as they are written.</li>
            </ul>
          </div>
        </div>
      </section>

      {/* What uze does, drawn: a screen of its own, the same height as the
          hero, so the illustration is never read against the headline or
          the next one. It carries no ground of its own and follows the
          theme. */}
      <section className="flex w-full max-w-[1296px] min-h-[calc(100dvh_-_var(--uze-banner-height)_-_3.5rem)] flex-col justify-center py-12">
        <SectionHeading
          eyebrow="1 · Package manager"
          title="One plugin, every agent."
          body="Add a marketplace, install a plugin, and each agent receives it through its own mechanism. Green arrived natively, amber was translated, and uze tells you which."
        />
        {/* As wide as the column, and no wider than the height left under
            the heading allows at the stage's 12:5, so the heading and the
            picture share one screen on a short desktop too. */}
        <div className="mx-auto mt-4 w-full" style={{ maxWidth: 'calc((100dvh - var(--uze-banner-height) - 3.5rem - 16rem) * 2.4)' }}>
          <HeroIllustration />
        </div>
      </section>

      {/* The workspace, drawn on the package illustration's stage so the two
          modules read as one set; the recording lives in its docs. */}
      <section className="flex w-full max-w-[1296px] min-h-[calc(100dvh_-_var(--uze-banner-height)_-_3.5rem)] flex-col justify-center py-12">
        <SectionHeading
          eyebrow="2 · Workspace"
          title="Several agents, one terminal."
          body="Each agent works on a branch in its own worktree. Read what it plans and what it changed, then land it with a rebase and your checks."
        />
        <div className="mx-auto mt-4 w-full" style={{ maxWidth: 'calc((100dvh - var(--uze-banner-height) - 3.5rem - 16rem) * 2.4)' }}>
          <WorkspaceIllustration />
        </div>
      </section>

      {/* What each half does for the reader. */}
      <section className="w-full max-w-5xl border-t border-line py-20 sm:py-24">
        <ul className="grid gap-x-16 gap-y-14 sm:grid-cols-2">
          {pillars.map((pillar) => (
            <li key={pillar.title}>
              <h3 className="font-mono text-lg font-semibold leading-snug text-ink">{pillar.title}</h3>
              <p className="mt-2.5 text-sm leading-relaxed text-muted">{pillar.body}</p>
              <Link
                href={pillar.href}
                className="mt-3.5 inline-block border-b border-accent/50 pb-0.5 font-mono text-xs text-ink transition-colors hover:border-accent hover:text-accent"
              >
                {pillar.link}
              </Link>
            </li>
          ))}
        </ul>
      </section>

      {/* What each harness receives. Generated from the integration code
          (web/lib/harness-matrix.json) — the same source the docs matrix is
          built from, so the landing page cannot claim a route the code
          stopped taking. */}
      <section className="w-full max-w-5xl border-t border-line py-20 sm:py-24">
        <h2 className="font-mono font-semibold text-ink">What each agent receives</h2>
        <p className="mt-1.5 max-w-[68ch] text-sm leading-relaxed text-muted">
          A check means the capability is delivered. The word beside it is how:{' '}
          <span className="text-ink">native</span> through the agent&apos;s own mechanism,{' '}
          <span className="text-ink">bridge</span> or <span className="text-ink">adapted</span>{' '}
          where uze preserves the semantics another way and reports that it did. Every route is
          derived from the integration that implements it.
        </p>

        <div className="mt-10 overflow-x-auto">
          <table className="w-full min-w-[38rem] border-collapse text-left">
            <thead>
              <tr className="border-b border-line">
                <th className="py-3 pe-4 font-mono text-xs font-normal text-muted">Agent</th>
                {columns.map(([key, label]) => (
                  <th key={key} className="px-3 py-3 font-mono text-xs font-normal text-muted">
                    {label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {matrix.harnesses.map((harness) => (
                <tr key={harness.name} className="border-b border-line/70">
                  <th scope="row" className="py-4 pe-4 font-normal">
                    <a
                      href={harness.url}
                      target="_blank"
                      rel="noreferrer noopener"
                      className="flex items-center gap-2.5 text-ink transition-colors hover:text-accent"
                    >
                      {harness.icon ? (
                        <img src={harness.icon} alt="" className="size-4 shrink-0 rounded-[2px]" />
                      ) : null}
                      <span className="font-mono text-sm font-semibold">{harness.name}</span>
                    </a>
                  </th>
                  {columns.map(([key]) => (
                    <td key={key} className="px-3 py-4">
                      <Route value={harness[key]} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <p className="mt-8 text-center text-sm text-muted">
          A dash means that route does not exist for the agent, and the capability arrives another
          way. {matrix.planned.join(', ')} are on the roadmap; cells appear when the integration
          lands.{' '}
          <Link
            href="/docs/reference/harnesses"
            className="text-ink underline underline-offset-4 hover:text-accent transition-colors"
          >
            The full matrix, per capability
          </Link>
          .
        </p>
      </section>

      {/* One column: two side by side fell out of step the moment one
          answer opened, and a question is read top to bottom anyway. */}
      <section id="faq" className="w-full max-w-5xl border-t border-line py-20 sm:py-24">
        <SectionHeading
          eyebrow="FAQ"
          title="Questions people ask first."
          body="The short answers. The docs have the long ones."
        />
        <div className="mx-auto mt-12 max-w-3xl border-t border-line">
          {/* The answer that undoes the misreading starts open; a reader
              who only skims sees it without a click. */}
          {faq.map((item, index) => (
            <details key={item.q} open={index === 0} className="group border-b border-line">
              <summary className="flex cursor-pointer list-none items-center justify-between gap-6 py-5 font-mono text-[15px] font-semibold text-ink transition-colors hover:text-accent [&::-webkit-details-marker]:hidden">
                {item.q}
                <Plus
                  className="size-4 shrink-0 text-muted transition-transform duration-200 group-open:rotate-45 group-open:text-accent"
                  strokeWidth={1.75}
                  aria-hidden
                />
              </summary>
              <p className="max-w-[65ch] pb-6 pe-10 text-[15px] leading-relaxed text-muted">{item.a}</p>
            </details>
          ))}
        </div>
        <p className="mt-8 text-center text-sm text-muted">
          Something else?{' '}
          <Link href="/docs/reference/faq" className="text-ink underline underline-offset-4 hover:text-accent transition-colors">
            Read the full FAQ
          </Link>{' '}
          or{' '}
          <Link href="https://github.com/uze-sh/uze/discussions" className="text-ink underline underline-offset-4 hover:text-accent transition-colors">
            ask on GitHub
          </Link>
          .
        </p>
      </section>

      <section className="w-full max-w-5xl border-t border-line py-24 sm:py-28 text-center">
        <h2 className="font-mono text-2xl font-bold tracking-tight text-ink">
          Set it up once, on this machine.
        </h2>
        <p className="mx-auto mt-3 max-w-[52ch] text-sm leading-relaxed text-muted">
          uze finds the coding agents you already have, installs any you want through the
          vendor&apos;s own installer, and reports what it could not do rather than guessing.
        </p>
        <div className="mt-10 flex flex-wrap items-center justify-center gap-4 font-mono text-xs">
          <Link
            href="/docs"
            className="border border-ink bg-ink px-5 py-2.5 text-paper transition-opacity hover:opacity-85"
          >
            Get started
          </Link>
          <Link
            href="/docs/plugins/creating"
            className="border border-line px-5 py-2.5 text-ink transition-colors hover:bg-surface"
          >
            Write a plugin
          </Link>
          <Link
            href="https://github.com/uze-sh/uze"
            className="inline-flex items-center gap-2 border border-line px-5 py-2.5 text-ink transition-colors hover:bg-surface"
          >
            <svg viewBox="0 0 16 16" className="size-3.5" fill="currentColor" aria-hidden="true">
              <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z" />
            </svg>
            Browse the source
          </Link>
        </div>
      </section>

      <footer className="w-full max-w-5xl border-t border-line py-14 text-center">
        <p className="text-[11px] font-mono text-muted">
          <UzeMark className="mr-2 inline-block size-[0.85em] align-middle text-accent" />
          Built with 🖤 by{' '}
          <a href="https://hiukky.com" className="text-ink hover:text-accent transition-colors">
            Romullo (@hiukky)
          </a>
        </p>
        <div className="mt-6">
          <TrademarkNotice />
        </div>
      </footer>
    </main>
  );
}
