import Link from 'next/link';
import { Archivo } from 'next/font/google';
import { Plus } from 'lucide-react';
import { InstallTabs } from '@/components/install-tabs';
import { WorkspaceIllustration } from '@/components/workspace-illustration';
import { TrademarkNotice } from '@/components/trademark-notice';
import { UzeMark } from '@/components/uze-mark';
import { faq, pillars } from '@/lib/home-content';
import { Bay } from '@/components/lab/patchbay/bay';
import { JackInline } from '@/components/lab/patchbay/jack';
import { Sheet } from '@/components/lab/patchbay/sheet';

// Exploration only: Archivo comes from Google here, which the root layout
// avoids on purpose (it self-hosts IBM's hinted Plex builds, see
// app/layout.tsx). If this direction wins, the display face is self-hosted
// the same way. The width axis is the point of choosing it: at 94% it sets
// like the engraved labels on a patch panel.
const archivo = Archivo({ subsets: ['latin'], axes: ['wdth'], variable: '--font-display', display: 'swap' });

// Tokens for this page only. A cool, slightly blue white under a cobalt
// accent, and a blue-black dark ground rather than a neutral one; the
// illustrations and the fumadocs chrome read the same variables, so the
// whole viewport changes with them.
const tokens = `
:root {
  --color-paper: #f3f5fa;
  --color-ink: #0d1333;
  --color-muted: #5a6386;
  --color-line: #d3d9ea;
  --color-surface: #e7ebf6;
  --color-accent: #2743ee;
  --color-warn: #9a5b00;
  --color-danger: #b8374a;
  --pb-on-accent: #ffffff;
}
.dark {
  --color-paper: #070b1f;
  --color-ink: #edf0fa;
  --color-muted: #99a3c6;
  --color-line: #1c2448;
  --color-surface: #0f1536;
  --color-accent: #6f86ff;
  --color-warn: #e3a83b;
  --color-danger: #f27285;
  --pb-on-accent: #070b1f;
}

.pb-display {
  font-family: var(--font-display), var(--font-body), sans-serif;
  font-stretch: 94%;
}
.pb-h1 {
  font-size: clamp(2.5rem, 6.4vw, 4.75rem);
  line-height: 0.98;
  letter-spacing: -0.03em;
  font-weight: 600;
  font-stretch: 92%;
  text-wrap: balance;
}
.pb-h2 {
  font-size: clamp(1.5rem, 2.6vw, 2rem);
  line-height: 1.1;
  letter-spacing: -0.02em;
  font-weight: 600;
}
.pb-label {
  font-size: 0.8125rem;
  font-weight: 500;
  color: var(--color-muted);
}
.pb a:focus-visible,
.pb button:focus-visible,
.pb summary:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: 3px;
}
.pb-link {
  transition: color 150ms;
}
.pb-link:hover {
  color: var(--color-accent);
}
.pb-primary,
.pb-secondary {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
  padding: 0.7rem 1.1rem;
  font-size: 0.9375rem;
  font-weight: 500;
  line-height: 1;
  white-space: nowrap;
}
.pb-primary {
  background: var(--color-accent);
  color: var(--pb-on-accent);
  transition: filter 150ms;
}
.pb-primary:hover {
  filter: brightness(1.08);
}
.pb-secondary {
  border: 1px solid var(--color-line);
  color: var(--color-ink);
  transition: border-color 150ms, color 150ms;
}
.pb-secondary:hover {
  border-color: var(--color-accent);
  color: var(--color-accent);
}

/* The rack: a band in the surface colour running edge to edge, with the
   install module and the bay on it. Everything drawn on it fills its holes
   with the band's own ground. */
.pb-rack {
  --pb-ground: var(--color-surface);
  background: var(--pb-ground);
}
.pb-module {
  position: relative;
  background: var(--color-paper);
  border: 1px solid var(--color-line);
  border-radius: 4px;
}
.pb-module .pb-jack-out {
  position: absolute;
  width: 1rem;
  height: 1rem;
  background: var(--pb-ground);
  border-radius: 50%;
}
/* Side by side from 64rem: the cable leaves the module's right edge. Below
   that the bay hangs under the module and the cables fan down from its
   bottom edge, at a width where the compact drawing's text stays readable. */
.pb-module .pb-jack-out { bottom: -0.5rem; left: calc(50% - 0.5rem); }
.pb-bay {
  width: 100%;
  height: auto;
  overflow: visible;
  color: var(--color-ink);
}
.pb-bay-wide { display: none; }
.pb-bay-compact { display: block; max-width: 24rem; margin: 0 auto; }
@media (min-width: 64rem) {
  .pb-module .pb-jack-out { left: auto; bottom: auto; right: -0.5rem; top: calc(50% - 0.5rem); }
  .pb-bay-wide { display: block; }
  .pb-bay-compact { display: none; }
}
.pb-column {
  font-family: var(--font-display), var(--font-body), sans-serif;
  font-stretch: 94%;
  font-size: 12px;
  font-weight: 500;
  fill: var(--color-muted);
}
.pb-name {
  font-family: var(--font-display), var(--font-body), sans-serif;
  font-stretch: 94%;
  font-weight: 600;
  fill: var(--color-ink);
}
.pb-branch {
  font-family: var(--font-ui-mono), ui-monospace, monospace;
  fill: var(--color-muted);
}
.pb-ring {
  fill: var(--pb-ground, var(--color-paper));
  stroke: var(--color-line);
  stroke-width: 1.5;
}
[data-lit] .pb-ring { stroke: var(--color-accent); }
.pb-pin { fill: var(--color-accent); }
.pb-cable-sleeve {
  fill: none;
  stroke: var(--pb-ground, var(--color-paper));
  stroke-width: 6;
}
.pb-cable {
  fill: none;
  stroke: var(--color-accent);
  stroke-width: 2;
  stroke-linecap: round;
  stroke-dasharray: 1;
  transition: stroke-width 150ms;
}
.pb-agent:hover .pb-cable { stroke-width: 3.5; }
.pb-lane-bed { stroke: var(--color-line); stroke-width: 2; }
.pb-lane { stroke: var(--color-accent); stroke-width: 2; }
.pb-commit {
  fill: var(--pb-ground, var(--color-paper));
  stroke: var(--color-accent);
  stroke-width: 1.5;
}
.pb-rail { stroke: var(--color-ink); stroke-width: 2; stroke-linecap: round; }

/* The one sequence: cables draw, jacks light, lanes extend, branches appear.
   Every rule below states only where a thing starts; the end state is the
   plain style above, which is all a reader with reduced motion gets. */
@keyframes pb-draw { from { stroke-dashoffset: 1; } }
@keyframes pb-unlit { from { stroke: var(--color-line); } }
@keyframes pb-extend { from { transform: scaleX(0); } }
@keyframes pb-appear { from { opacity: 0; } }
.pb-cable {
  animation: pb-draw 0.7s cubic-bezier(0.4, 0, 0.2, 1) calc(0.2s + var(--i) * 0.14s) backwards;
}
.pb-agent .pb-ring {
  animation: pb-unlit 0.3s calc(0.9s + var(--i) * 0.14s) backwards;
}
.pb-agent .pb-pin,
.pb-agent .pb-name,
.pb-agent svg {
  animation: pb-appear 0.3s calc(0.9s + var(--i) * 0.14s) backwards;
}
.pb-lane {
  animation: pb-extend 0.8s cubic-bezier(0.4, 0, 0.2, 1) calc(1.25s + var(--i) * 0.12s) backwards;
}
.pb-commit,
.pb-branch {
  animation: pb-appear 0.4s calc(1.9s + var(--i) * 0.12s) backwards;
}
@media (prefers-reduced-motion: reduce) {
  .pb-bay * { animation: none !important; }
}
`;

export default function PatchbayPage() {
  return (
    <main className={`pb ${archivo.variable} flex w-full flex-1 flex-col font-sans`}>
      <style>{tokens}</style>

      {/* The words, set wide and left, as the top of the rack. */}
      <section className="mx-auto grid w-full max-w-6xl gap-x-16 gap-y-7 px-6 pt-14 pb-12 sm:pt-20 sm:pb-14 lg:grid-cols-[minmax(0,1fr)_26rem] lg:items-end">
        <h1 className="pb-display pb-h1 max-w-[14ch] text-ink">The package manager for coding agents.</h1>
        <div>
          <p className="max-w-[42rem] text-pretty text-lg leading-relaxed text-muted sm:text-xl">
            Install skills, MCP servers, hooks and <code className="font-mono text-ink">AGENTS.md</code>{' '}
            into Claude Code, Codex, OpenCode and Antigravity, then run them side by side, each in
            its own worktree.
          </p>
          {/* Said once, here, because it is the first thing a reader gets
              wrong: a tool "for coding agents" reads as one more coding agent. */}
          <p className="pb-display mt-6 inline-flex items-center gap-2.5 text-[15px] font-medium text-ink">
            <JackInline lit={false} className="size-4" />
            Not an agent. No model, no API key.
          </p>
        </div>
      </section>

      {/* The rack: the install command is the input module, and the bay is
          what it is patched into. */}
      <section className="pb-rack w-full" aria-label="How a plugin reaches your agents">
        <div className="mx-auto grid w-full max-w-6xl gap-10 px-6 py-10 sm:py-14 lg:grid-cols-[21rem_minmax(0,1fr)] lg:items-center lg:gap-0">
          <div className="pb-module p-5">
            <p className="pb-label">Install, then add a plugin</p>
            <div className="mt-3">
              <InstallTabs />
            </div>
            <div className="mt-5 flex flex-wrap gap-3">
              <Link href="/docs/quickstart" className="pb-primary">
                Get started
              </Link>
              <Link href="/docs/plugins/delivery" className="pb-secondary">
                How delivery works
              </Link>
            </div>
            <span className="pb-jack-out" aria-hidden>
              <JackInline lit className="size-4" />
            </span>
          </div>
          <Bay />
        </div>
      </section>

      {/* What each harness receives, from the same generated matrix the
          docs use, so the page cannot claim a route the code stopped taking. */}
      <section className="mx-auto w-full max-w-6xl px-6 py-20 sm:py-24">
        <h2 className="pb-display pb-h2 text-ink">What each agent receives</h2>
        <p className="mt-3 max-w-[64ch] text-base leading-relaxed text-muted">
          A lit jack means the capability is delivered; the word beside it is how.{' '}
          <span className="text-ink">native</span> is the agent&apos;s own mechanism,{' '}
          <span className="text-ink">bridge</span> and <span className="text-ink">adapted</span> are
          where uze keeps the semantics another way and reports that it did. Every cell is derived
          from the integration that implements it.
        </p>
        <div className="mt-10">
          <Sheet />
        </div>
      </section>

      {/* The lanes, in use: the workspace drawn on this page's palette. */}
      <section className="mx-auto w-full max-w-6xl px-6 py-8 sm:py-12">
        <h2 className="pb-display pb-h2 text-ink">Several agents, one terminal.</h2>
        <p className="mt-3 max-w-[64ch] text-base leading-relaxed text-muted">
          Each agent works on a branch in its own worktree. Read what it plans and what it changed,
          then land it with a rebase and your checks.
        </p>
        <div className="mt-8">
          <WorkspaceIllustration />
        </div>
      </section>

      {/* What each half does for the reader. Plain text in two columns: the
          titles already carry the structure. */}
      <section className="mx-auto w-full max-w-6xl px-6 py-20 sm:py-24">
        <h2 className="pb-display pb-h2 text-ink">A package manager and a workspace.</h2>
        <p className="mt-3 max-w-[64ch] text-base leading-relaxed text-muted">
          Each works without the other, and both ship in one Rust binary under Apache 2.0.
        </p>
        <ul className="mt-12 grid gap-x-14 gap-y-12 sm:grid-cols-2">
          {pillars.map((pillar) => (
            <li key={pillar.title} className="max-w-[46ch]">
              <h3 className="pb-display text-xl font-semibold leading-snug text-ink">{pillar.title}</h3>
              <p className="mt-3 text-[15px] leading-relaxed text-muted">{pillar.body}</p>
              <Link href={pillar.href} className="pb-link mt-4 inline-block text-sm text-ink underline underline-offset-4">
                {pillar.link}
              </Link>
            </li>
          ))}
        </ul>
      </section>

      {/* One column of questions; the answer that undoes the misreading
          starts open so a skimmer sees it without a click. */}
      <section id="faq" className="mx-auto w-full max-w-6xl px-6 py-20 sm:py-24">
        <div className="grid gap-10 lg:grid-cols-[18rem_1fr] lg:gap-16">
          <div>
            <h2 className="pb-display pb-h2 text-ink">Questions people ask first.</h2>
            <p className="mt-3 text-base leading-relaxed text-muted">
              The short answers.{' '}
              <Link href="/docs/reference/faq" className="pb-link text-ink underline underline-offset-4">
                The docs have the long ones
              </Link>
              .
            </p>
          </div>
          <div className="border-t border-line">
            {faq.map((item, index) => (
              <details key={item.q} open={index === 0} className="group border-b border-line">
                <summary className="pb-display flex cursor-pointer list-none items-center justify-between gap-6 py-5 text-[17px] font-semibold text-ink [&::-webkit-details-marker]:hidden">
                  {item.q}
                  <Plus
                    className="size-4 shrink-0 text-muted transition-transform duration-200 group-open:rotate-45 group-open:text-accent"
                    strokeWidth={1.75}
                    aria-hidden
                  />
                </summary>
                <p className="max-w-[62ch] pb-6 pe-10 text-[15px] leading-relaxed text-muted">{item.a}</p>
              </details>
            ))}
          </div>
        </div>
      </section>

      {/* Sign-off: the same install, and the three places to go next. */}
      <section className="pb-rack w-full">
        <div className="mx-auto grid w-full max-w-6xl gap-8 px-6 py-16 sm:py-20 lg:grid-cols-[1fr_21rem] lg:items-center">
          <div>
            <h2 className="pb-display pb-h2 text-ink">Set it up once, on this machine.</h2>
            <p className="mt-3 max-w-[52ch] text-base leading-relaxed text-muted">
              uze finds the coding agents you already have, installs any you want through the
              vendor&apos;s own installer, and reports what it could not do rather than guessing.
            </p>
            <div className="mt-7 flex flex-wrap gap-3">
              <Link href="/docs" className="pb-primary">
                Read the docs
              </Link>
              <Link href="/docs/plugins/creating" className="pb-secondary">
                Write a plugin
              </Link>
              <Link href="https://github.com/uze-sh/uze" className="pb-secondary gap-2">
                <svg viewBox="0 0 16 16" className="size-3.5" fill="currentColor" aria-hidden="true">
                  <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z" />
                </svg>
                Browse the source
              </Link>
            </div>
          </div>
          <div className="pb-module p-5">
            <p className="pb-label">Install</p>
            <div className="mt-3">
              <InstallTabs />
            </div>
          </div>
        </div>
      </section>

      <footer className="mx-auto w-full max-w-6xl px-6 py-12">
        <p className="text-sm text-muted">
          <UzeMark className="mr-2 inline-block size-[0.85em] align-middle text-accent" />
          Built by{' '}
          <a href="https://hiukky.com" className="pb-link text-ink">
            Romullo (@hiukky)
          </a>
        </p>
        <div className="mt-5 [&>p]:mx-0">
          <TrademarkNotice />
        </div>
      </footer>
    </main>
  );
}
