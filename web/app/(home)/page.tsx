import Link from 'next/link';
import { Plus } from 'lucide-react';
import { InstallTabs } from '@/components/install-tabs';
import { HarnessMark, harnessMarks, homepageOf } from '@/components/harness-marks';
import { TrademarkNotice } from '@/components/trademark-notice';
import { faq } from '@/lib/home-content';
import { ConsoleSession } from '@/components/home/session';

export default function HomePage() {
  return (
    <main className="flex flex-1 flex-col items-center px-4 font-sans sm:px-6">

      {/* Two columns: what uze is and how to get it on the left, the session
          that shows it on the right, so the claim and the proof are read
          side by side. The stream is the page's own terminal, with no window
          drawn around it. */}
      <section className="w-full max-w-[84rem] pt-12 sm:pt-20">
        <div className="grid gap-x-14 gap-y-10 lg:grid-cols-[minmax(0,30rem)_minmax(0,1fr)] lg:items-center xl:grid-cols-[minmax(0,36rem)_minmax(0,1fr)] xl:gap-x-20">
          <div>
            {/* Each sentence on a line of its own, so a break never lands
                mid-thought. */}
            <h1 className="text-[2.75rem] font-semibold leading-[0.98] tracking-[-0.045em] text-ink sm:text-[4rem] lg:text-[3.1rem] xl:text-[3.75rem]">
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
          <div className="min-w-0">
            <ConsoleSession />
          </div>
        </div>

        {/* Under the session it describes, not under the whole hero. */}
        <div className="mt-5 flex flex-wrap items-start justify-between gap-x-8 gap-y-4 lg:ml-[calc(30rem+3.5rem)] xl:ml-[calc(36rem+5rem)]">
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
          <ul className="flex flex-wrap items-center gap-x-6 gap-y-2">
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
      <section id="faq" className="w-full max-w-[84rem] py-20 sm:py-24">
        <div className="grid gap-x-16 gap-y-8 lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)]">
          <header>
            <h2 className="text-2xl font-semibold tracking-tight text-ink sm:text-[1.75rem]">
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
          <div className="border-t border-line">
            {faq.map((item, index) => (
              <details key={item.q} open={index === 0} className="group border-b border-line">
                <summary className="flex cursor-pointer list-none items-center justify-between gap-6 py-4 text-[15px] font-semibold text-ink transition-colors hover:text-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent [&::-webkit-details-marker]:hidden">
                  {item.q}
                  <Plus
                    className="size-4 shrink-0 text-muted transition-transform duration-200 group-open:rotate-45 group-open:text-accent"
                    strokeWidth={1.75}
                    aria-hidden
                  />
                </summary>
                <p className="max-w-[65ch] pb-5 pe-10 text-[15px] leading-relaxed text-muted">{item.a}</p>
              </details>
            ))}
          </div>
        </div>
      </section>

      <section className="w-full max-w-[84rem] border-t border-line py-16 sm:py-20">
        <div className="grid gap-x-16 gap-y-6 lg:grid-cols-[minmax(0,1fr)_26rem] lg:items-end">
          <div className="max-w-[40rem]">
            <h2 className="text-2xl font-semibold tracking-tight text-ink sm:text-[1.75rem]">
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
