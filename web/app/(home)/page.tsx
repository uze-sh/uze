import Link from 'next/link';
import { Plus } from 'lucide-react';
import { InstallTabs } from '@/components/install-tabs';
import { HarnessMark, harnessMarks, homepageOf } from '@/components/harness-marks';
import { TrademarkNotice } from '@/components/trademark-notice';
import { faq } from '@/lib/home-content';
import { ConsoleSession } from '@/components/home/session';
import { SessionSummary } from '@/components/home/session-summary';
import { HeroStage } from '@/components/home/hero-stage';

export default function HomePage() {
  return (
    <main className="flex flex-1 flex-col items-center px-4 font-sans">
      {/* One grid with the header: fumadocs sizes its home layout by
          `--fd-layout-width` and insets the header by `px-4`, so every section
          takes that width less the same inset, and its edges meet the logo and
          the header's last control at every screen size. */}
      {/* One screen in two halves: the claim on top and the session
          peeking from below. Scrolling hands the screen to the session
          before the page moves on, so the proof gets the whole view and the
          claim never competes with it for width. */}
      <section className="w-full max-w-[calc(var(--fd-layout-width)-2rem)]">
        <HeroStage
          hero={
            /* Below `md` the hero is a screen of its own: the claim and the
               way to install it, with nothing competing for the first view. */
            <div className="flex min-h-[calc(100svh_-_var(--uze-banner-height)_-_3.5rem)] flex-col justify-center py-16 md:min-h-0 md:items-center md:py-0 md:text-center">
              {/* Each sentence on a line of its own, so a break never lands
                  mid-thought. */}
              <h1 className="text-[2.3rem] font-semibold leading-[0.98] tracking-[-0.045em] text-ink sm:text-[4rem] lg:text-[4.25rem]">
                <span className="block text-balance">Agents come and go.</span>
                <span className="block text-balance">Your work stays.</span>
              </h1>
              <p className="mt-6 max-w-[44rem] text-pretty text-[17px] leading-relaxed text-muted sm:text-[18px]">
                A layer between you and your agents: the same plugins and{' '}
                <code className="font-mono text-[0.95em] text-ink">AGENTS.md</code> in Claude Code,
                Codex, OpenCode and Antigravity, and a terminal to run them side by side.
              </p>
              {/* One row for the one thing to do: the command, and the way
                  into the docs at its height. The source is a click away in
                  the header and at the foot of the page. */}
              <div className="mt-7 flex w-full max-w-[34rem] flex-col gap-3 sm:flex-row sm:items-start">
                <div className="min-w-0 flex-1">
                  <InstallTabs />
                </div>
                <Link
                  href="/docs/quickstart"
                  className="inline-flex shrink-0 items-center justify-center px-5 py-2.5 font-mono text-[13px] font-semibold transition-opacity hover:opacity-85 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent sm:mt-[30px] sm:h-[46px]"
                  style={{ background: 'var(--color-accent)', color: 'var(--color-paper)' }}
                >
                  Get started
                </Link>
              </div>
            </div>
          }
          session={
            /* Stacked, the session is the next section rather than the
               hero's tail: a rule and a heading of its own, so it reads as
               "here is how it is used" and not as more of the same block. */
            <div className="min-w-0 border-t border-line py-20 md:border-0 md:py-0">
              <h2 className="mb-10 text-[1.75rem] font-semibold tracking-tight text-ink md:hidden">
                Three commands.
              </h2>
              {/* Always the dark palette: uze's terminal has no light theme, so
                  a session drawn on the page's light ground showed a uze that
                  does not exist. `.dark` on the block re-points the tokens for
                  it alone. Framed as a window: at the full width of the page a
                  bare black panel read as a hole in it, not as a terminal. */}
              <figure className="dark hidden overflow-hidden rounded-lg bg-paper text-ink shadow-[0_24px_60px_-20px_rgb(0_0_0/0.45)] ring-1 ring-black/10 md:block dark:ring-white/10">
                <div className="relative flex h-9 items-center bg-surface px-4">
                  <div className="flex gap-2" aria-hidden>
                    <span className="size-3 rounded-full bg-line" />
                    <span className="size-3 rounded-full bg-line" />
                    <span className="size-3 rounded-full bg-line" />
                  </div>
                  <figcaption className="absolute inset-x-0 text-center font-mono text-xs text-muted">
                    ~/project · uze
                  </figcaption>
                </div>
                <div className="px-5 pb-2 [&>div]:border-b-0">
                  <ConsoleSession />
                </div>
              </figure>
              <div className="md:hidden">
                <SessionSummary />
              </div>

              {/* Under the session it describes and shaped like the chapter bar
                  above it: the agents spread across the session's width, so the
                  session sits framed between the two. Only beside the full
                  session: the stacked summary names the agents in its own second
                  step and links the matrix itself. */}
              <div className="mt-8 hidden md:block">
                {/* "Works with", never "powered by": the marks are the agents uze
                    serves, not anything it is made of. */}
                <ul className="flex flex-wrap items-center justify-between gap-x-4 gap-y-3 px-5">
                  {harnessMarks.map((harness) => (
                    <li key={harness.name}>
                      <a
                        href={homepageOf(harness.name)}
                        target="_blank"
                        rel="noreferrer noopener"
                        className="flex items-center gap-2 whitespace-nowrap text-ink transition-colors hover:text-muted"
                      >
                        <HarnessMark icon={harness.icon} className="size-4" />
                        <span className="font-mono text-[13px] font-semibold">{harness.name}</span>
                      </a>
                    </li>
                  ))}
                </ul>
                <p className="mt-5 border-t border-line px-5 pt-5 text-center text-sm leading-relaxed text-balance text-muted">
                  Every route the agents report above is read from the integration that implements it.{' '}
                  <Link
                    href="/docs/reference/harnesses"
                    className="text-ink underline decoration-line underline-offset-4 transition-colors hover:decoration-ink"
                  >
                    The full matrix, per capability
                  </Link>
                  .
                </p>
              </div>
            </div>
          }
        />
      </section>

      {/* One column: a question is read top to bottom. The answer that undoes
          the misreading starts open, so a reader who only skims sees it. */}
      <section id="faq" className="w-full max-w-[calc(var(--fd-layout-width)-2rem)] border-t border-line py-20 sm:py-24 lg:mt-16">
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
                <div className="max-w-[65ch] pb-6 pe-10 text-[15px] leading-relaxed text-muted">{item.a}</div>
              </details>
            ))}
          </div>
        </div>
      </section>

      <section className="w-full max-w-[calc(var(--fd-layout-width)-2rem)] border-t border-line py-20">
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

      <footer className="w-full max-w-[calc(var(--fd-layout-width)-2rem)] border-t border-line py-10">
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
