import Link from 'next/link';
import { Schibsted_Grotesk } from 'next/font/google';
import { Plus } from 'lucide-react';
import { InstallTabs } from '@/components/install-tabs';
import { WorkspaceIllustration } from '@/components/workspace-illustration';
import { TrademarkNotice } from '@/components/trademark-notice';
import { UzeMark } from '@/components/uze-mark';
import { faq, pillars } from '@/lib/home-content';
import { ManifestTheme } from '@/components/lab/manifest/theme';
import { Listing } from '@/components/lab/manifest/listing';

// Exploration only: Google's build of Schibsted Grotesk, fetched at build
// time. The root layout self-hosts IBM Plex for the hinting reason written
// there, and a font that graduates from the lab would be self-hosted the
// same way.
const display = Schibsted_Grotesk({
  subsets: ['latin'],
  weight: 'variable',
  variable: '--font-display',
  display: 'swap',
});

export default function ManifestPage() {
  return (
    <main className={`mf ${display.variable} flex flex-1 flex-col items-center px-4 sm:px-6`}>
      <ManifestTheme />

      {/* The headline and the install side by side, so the file beneath
          them is the first thing below the fold rather than the third. */}
      <section className="w-full max-w-[76rem] pt-14 sm:pt-20 lg:pt-24">
        <div className="grid gap-10 lg:grid-cols-2 lg:gap-12">
          <h1 className="mf-display text-balance text-[2.75rem] text-ink sm:text-[3.75rem] lg:text-[5rem]">
            The package manager for coding agents.
          </h1>
          <div className="max-w-[36rem] lg:pt-3">
            <p className="text-pretty text-[1.0625rem] leading-[1.55] text-muted sm:text-lg">
              Install skills, MCP servers, hooks and{' '}
              <code className="font-mono text-[0.92em] text-ink">AGENTS.md</code> into Claude Code,
              Codex, OpenCode and Antigravity, then run them side by side, each in its own worktree.
            </p>
            <div className="mt-7 flex flex-col gap-3 sm:flex-row sm:items-start">
              <div className="min-w-0 flex-1">
                <InstallTabs />
              </div>
              <Link
                href="/docs/quickstart"
                className="inline-flex shrink-0 items-center justify-center rounded-md bg-accent px-5 py-2.5 text-[0.9375rem] font-semibold text-paper transition-opacity hover:opacity-90 sm:mt-[1.85rem]"
              >
                Quickstart
              </Link>
            </div>
            {/* The misreading, answered once, where the eye lands after the
                install line. */}
            <p className="mt-6 flex items-center gap-2 text-[0.9375rem] font-medium text-ink">
              <UzeMark className="size-3.5 shrink-0 text-accent" />
              Not an agent. No model, no API key.
            </p>
          </div>
        </div>

        <div className="mt-14 sm:mt-16 lg:mt-20">
          <Listing />
        </div>
      </section>

      {/* What each half does for the reader, in their words. */}
      <section className="w-full max-w-[76rem] pt-28 sm:pt-36">
        <h2 className="mf-title max-w-[22ch] text-[2rem] text-ink sm:text-[2.75rem]">
          One copy on the machine, one file in the repo.
        </h2>
        <ul className="mt-14 grid gap-x-16 gap-y-12 sm:grid-cols-2">
          {pillars.map((pillar) => (
            <li key={pillar.title} className="max-w-[34rem]">
              <h3 className="mf-title text-[1.3125rem] text-ink">{pillar.title}</h3>
              <p className="mt-3 text-[0.9875rem] leading-[1.6] text-muted">{pillar.body}</p>
              <Link
                href={pillar.href}
                className="mt-3 inline-block text-[0.9375rem] font-medium text-ink underline decoration-line underline-offset-[5px] transition-colors hover:decoration-accent"
              >
                {pillar.link}
              </Link>
            </li>
          ))}
        </ul>
      </section>

      {/* The other half of the binary, drawn with the page's own palette. */}
      <section className="w-full max-w-[76rem] pt-28 sm:pt-36">
        <div className="grid gap-6 lg:grid-cols-[minmax(0,7fr)_minmax(0,5fr)] lg:gap-16">
          <h2 className="mf-title max-w-[18ch] text-[2rem] text-ink sm:text-[2.75rem]">
            Then run them side by side, each in its own worktree.
          </h2>
          <p className="max-w-[34rem] text-[1.0625rem] leading-[1.55] text-muted lg:pt-3">
            The workspace gives every agent a tab, a branch and a checkout of its own. You read what
            it changed, and when the work is ready one key rebases it, runs your checks and opens the
            pull request.
          </p>
        </div>
        <div className="mx-auto mt-12 w-full max-w-[1296px]">
          <WorkspaceIllustration />
        </div>
      </section>

      <section id="faq" className="w-full max-w-[76rem] pt-28 sm:pt-36">
        <div className="grid gap-10 lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)] lg:gap-16">
          <h2 className="mf-title text-[2rem] text-ink sm:text-[2.75rem]">Questions people ask first.</h2>
          <div className="border-t border-line">
            {faq.map((item, index) => (
              <details key={item.q} open={index === 0} className="group border-b border-line">
                <summary className="flex cursor-pointer list-none items-center justify-between gap-6 py-4 text-[1.0625rem] font-semibold text-ink transition-colors hover:text-accent">
                  {item.q}
                  <Plus
                    className="size-4 shrink-0 text-muted transition-transform duration-200 group-open:rotate-45 group-open:text-accent"
                    strokeWidth={1.75}
                    aria-hidden
                  />
                </summary>
                <p className="max-w-[62ch] pb-5 pe-10 text-[0.9875rem] leading-[1.6] text-muted">{item.a}</p>
              </details>
            ))}
            <p className="mt-6 text-[0.9375rem] text-muted">
              Something else?{' '}
              <Link href="/docs/reference/faq" className="text-ink underline decoration-line underline-offset-4 hover:decoration-accent">
                Read the full FAQ
              </Link>{' '}
              or{' '}
              <Link href="https://github.com/uze-sh/uze/discussions" className="text-ink underline decoration-line underline-offset-4 hover:decoration-accent">
                ask on GitHub
              </Link>
              .
            </p>
          </div>
        </div>
      </section>

      <section className="w-full max-w-[76rem] pt-28 pb-24 sm:pt-36">
        <div className="grid gap-8 lg:grid-cols-[minmax(0,7fr)_minmax(0,5fr)] lg:gap-16">
          <h2 className="mf-title max-w-[18ch] text-[2rem] text-ink sm:text-[2.75rem]">
            Set it up once, on this machine.
          </h2>
          <div className="lg:pt-3">
            <p className="max-w-[34rem] text-[1.0625rem] leading-[1.55] text-muted">
              uze finds the coding agents you already have, installs any you want through the
              vendor&apos;s own installer, and reports what it could not do rather than guessing.
              One Rust binary, Apache 2.0, Linux, macOS and Windows.
            </p>
            <div className="mt-8 flex flex-wrap gap-3 text-[0.9375rem] font-semibold">
              <Link href="/docs" className="rounded-md bg-accent px-5 py-2.5 text-paper transition-opacity hover:opacity-90">
                Get started
              </Link>
              <Link href="/docs/plugins/creating" className="rounded-md border border-line px-5 py-2.5 text-ink transition-colors hover:border-ink">
                Write a plugin
              </Link>
              <Link href="https://github.com/uze-sh/uze" className="rounded-md border border-line px-5 py-2.5 text-ink transition-colors hover:border-ink">
                Browse the source
              </Link>
            </div>
          </div>
        </div>
      </section>

      <footer className="w-full max-w-[76rem] border-t border-line py-12">
        <p className="text-[0.875rem] text-muted">
          <UzeMark className="mr-2 inline-block size-[0.85em] align-middle text-accent" />
          Built with 🖤 by{' '}
          <a href="https://hiukky.com" className="text-ink transition-colors hover:text-accent">
            Romullo (@hiukky)
          </a>
        </p>
        <div className="mt-6 [&>p]:mx-0">
          <TrademarkNotice />
        </div>
      </footer>
    </main>
  );
}
