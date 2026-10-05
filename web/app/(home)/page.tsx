import Link from 'next/link';
import { Plus } from 'lucide-react';
import { InstallTabs } from '@/components/install-tabs';
import { HarnessMark, harnessMarks, homepageOf } from '@/components/harness-marks';
import { TrademarkNotice } from '@/components/trademark-notice';
import { faq } from '@/lib/home-content';
import { ConsoleSession } from '@/components/home/session';
import { SessionSummary } from '@/components/home/session-summary';

export default function HomePage() {
  return (
    <main className="flex flex-1 flex-col items-center px-4 font-sans sm:px-6">
      {/* Two columns: what uze is and how to get it on the left, the session
          that shows it on the right, so the claim and the proof are read
          side by side. The stream is the page's own terminal, with no window
          drawn around it. */}
      <section className="w-full max-w-[84rem] lg:pt-20">
        <div className="grid gap-x-14 lg:grid-cols-[minmax(0,30rem)_minmax(0,1fr)] lg:items-center xl:grid-cols-[minmax(0,36rem)_minmax(0,1fr)] xl:gap-x-20">
          {/* Below the two-column width the hero is a screen of its own:
              the claim and the way to install it, with nothing competing
              for the first view. */}
          <div className="flex min-h-[calc(100svh_-_var(--uze-banner-height)_-_3.5rem)] flex-col justify-center py-16 lg:min-h-0 lg:py-0">
            {/* Each sentence on a line of its own, so a break never lands
                mid-thought. */}
            <h1 className="text-[2.3rem] font-semibold leading-[0.98] tracking-[-0.045em] text-ink sm:text-[4rem] lg:text-[3.1rem] xl:text-[3.75rem]">
              <span className="block text-balance">Agents come and go.</span>
              <span className="block text-balance">Your work stays.</span>
            </h1>
            <p className="mt-7 max-w-[34rem] text-pretty text-[17px] leading-relaxed text-muted sm:text-[18px]">
              A layer between you and your agents: the same plugins and{' '}
              <code className="font-mono text-[0.95em] text-ink">AGENTS.md</code> in Claude Code,
              Codex, OpenCode and Antigravity, and a terminal to run them side by side.
            </p>
            <div className="mt-8">
              <InstallTabs />
            </div>
            <p className="mt-5 font-mono text-[13px] text-ink">
              <span className="text-success" aria-hidden>
                ✓{' '}
              </span>
              Not an agent. No model, no API key.
            </p>
          </div>
          {/* Stacked, the session is the next section rather than the
              hero's tail: a rule and a heading of its own, so it reads as
              "here is how it is used" and not as more of the same block. */}
          <div className="min-w-0 border-t border-line py-20 lg:border-0 lg:py-0">
            <h2 className="mb-10 text-[1.75rem] font-semibold tracking-tight text-ink lg:hidden">
              Three commands.
            </h2>
            {/* Always the dark palette: uze's terminal has no light theme, so a
                session drawn on the page's light ground showed a uze that
                does not exist. `.dark` on the block re-points the tokens for
                it alone. A plain panel, no window chrome. */}
            <div className="dark hidden bg-paper px-5 py-1 text-ink md:block">
              <ConsoleSession />
            </div>
            <div className="md:hidden">
              <SessionSummary />
            </div>
          </div>
        </div>

        {/* Under the session it describes, not under the whole hero. */}
        {/* Only beside the full session: the stacked summary names the
            agents in its own second step and links the matrix itself. */}
        <div className="mt-8 hidden flex-wrap items-start justify-between gap-x-8 gap-y-6 md:flex lg:mt-5 lg:ml-[calc(30rem+3.5rem)] xl:ml-[calc(36rem+5rem)]">
          <p className="max-w-[60ch] text-sm leading-relaxed text-muted">
            Every route the agents report above is read from the integration that implements it.{' '}
            <Link
              href="/docs/reference/harnesses"
              className="text-ink underline decoration-line underline-offset-4 transition-colors hover:decoration-ink"
            >
              The full matrix, per capability
            </Link>
            .
          </p>
          {/* "Works with", never "powered by": the marks are the agents uze
              serves, not anything it is made of. */}
          <ul className="grid grid-cols-2 gap-x-6 gap-y-3 sm:flex sm:flex-wrap sm:items-center">
            {harnessMarks.map((harness) => (
              <li key={harness.name}>
                <a
                  href={homepageOf(harness.name)}
                  target="_blank"
                  rel="noreferrer noopener"
                  className="flex items-center gap-2 text-ink transition-colors hover:text-muted"
                >
                  <HarnessMark icon={harness.icon} className="size-4" />
                  <span className="font-mono text-[13px] font-semibold">{harness.name}</span>
                </a>
              </li>
            ))}
          </ul>
        </div>
      </section>

      {/* One column: a question is read top to bottom. The answer that undoes
          the misreading starts open, so a reader who only skims sees it. */}
      <section id="faq" className="w-full max-w-[84rem] border-t border-line py-20 sm:py-24 lg:mt-16">
        <div className="grid gap-x-16 gap-y-8 lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)]">
          <header>
            <h2 className="text-[1.75rem] font-semibold tracking-tight text-ink">
              Questions people ask first
            </h2>
            <p className="mt-3 text-[15px] leading-relaxed text-muted">
              The short answers. The docs have the long ones.
            </p>
            <p className="mt-5 text-sm text-muted">
              Something else?{' '}
              <Link
                href="/docs/reference/faq"
                className="text-ink underline decoration-line underline-offset-4 transition-colors hover:decoration-ink"
              >
                Read the full FAQ
              </Link>{' '}
              or{' '}
              <Link
                href="https://github.com/uze-sh/uze/discussions"
                className="text-ink underline decoration-line underline-offset-4 transition-colors hover:decoration-ink"
              >
                ask on GitHub
              </Link>
              .
            </p>
          </header>
          {/* Spaced, not ruled: the section's own divider already says where
              the list starts, and a hairline under every question stacked
              into a ladder of lines on a phone. */}
          <div className="space-y-1">
            {faq.map((item, index) => (
              <details key={item.q} open={index === 0} className="group">
                <summary className="flex cursor-pointer list-none items-center justify-between gap-6 py-4 text-[15px] font-semibold text-ink transition-colors hover:text-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent [&::-webkit-details-marker]:hidden">
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
        </div>
      </section>

      <section className="w-full max-w-[84rem] border-t border-line py-20">
        <div className="grid gap-x-16 gap-y-6 lg:grid-cols-[minmax(0,1fr)_26rem] lg:items-end">
          <div className="max-w-[40rem]">
            <h2 className="text-[1.75rem] font-semibold tracking-tight text-ink">
              Set it up once, on this machine.
            </h2>
            <p className="mt-3 text-[15px] leading-relaxed text-muted">
              uze finds the coding agents you already have, installs any you want through the vendor&apos;s
              own installer, and reports what it could not do rather than guessing. One Rust binary, open
              source under the Apache License 2.0.
            </p>
            <div className="mt-6 flex flex-wrap gap-3 font-mono text-[13px]">
              <Link
                href="/docs/quickstart"
                className="inline-flex items-center px-4 py-2 font-semibold transition-opacity hover:opacity-85 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
                style={{ background: 'var(--color-accent)', color: 'var(--color-paper)' }}
              >
                Get started
              </Link>
              <Link
                href="https://github.com/uze-sh/uze"
                className="inline-flex items-center px-4 py-2 text-ink transition-colors hover:text-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
                style={{ boxShadow: 'inset 0 0 0 1px var(--color-line)' }}
              >
                Source on GitHub
              </Link>
            </div>
          </div>
          <InstallTabs />
        </div>
      </section>

      <footer className="w-full max-w-[84rem] border-t border-line py-10">
        <p className="font-mono text-[11px] text-muted">
          Built by{' '}
          <a href="https://hiukky.com" className="text-ink transition-colors hover:text-muted">
            Romullo (@hiukky)
          </a>
          .
        </p>
        <div className="mt-5 [&>p]:mx-0">
          <TrademarkNotice />
        </div>
      </footer>
    </main>
  );
}
