import type { ReactNode } from 'react';
import { UzeMark } from '@/components/uze-mark';
import { HarnessMark, harnessMarks } from '@/components/harness-marks';

// Where uze sits, drawn so it cannot be read as a fifth agent. Every layer is
// something the reader already owns (plugins, agents, worktrees, a repo), and
// uze appears only on the steps between them, as the verb: it installs,
// isolates and lands.

const PLUGIN_PARTS = ['skills', 'MCP servers', 'hooks', 'subagents', 'AGENTS.md'];
const BRANCHES = ['feat/parser', 'fix/login', 'docs/readme', 'refactor/api'];

function Layer({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-2 sm:grid-cols-[8rem_1fr] sm:items-center sm:gap-6">
      <span className="text-[11px] uppercase tracking-[0.08em] text-muted">{label}</span>
      <div>{children}</div>
    </div>
  );
}

function Step({ children }: { children: ReactNode }) {
  return (
    <div className="grid sm:grid-cols-[8rem_1fr] sm:gap-6">
      <span className="hidden sm:block" />
      <div className="flex items-stretch gap-3 py-1.5">
        <span className="ml-4 w-px bg-accent/60" />
        <span className="inline-flex flex-wrap items-center gap-x-2 gap-y-0.5 py-2.5 text-xs text-ink">
          <span className="inline-flex items-center gap-1.5 font-semibold text-accent">
            <UzeMark className="size-3" />
            uze
          </span>
          {children}
        </span>
      </div>
    </div>
  );
}

const quad = 'grid grid-cols-2 gap-2 sm:grid-cols-4';

export function AroundIllustration() {
  return (
    <figure
      className="mx-auto w-full max-w-4xl font-mono"
      aria-label="A plugin of skills, MCP servers, hooks, subagents and AGENTS.md; uze installs it into Claude Code, Codex, OpenCode and Antigravity; uze gives each agent its own git worktree; and uze lands the finished work in your repository with a rebase, your checks and a pull request."
    >
      <div aria-hidden>
        <Layer label="A plugin">
          <div className="flex flex-wrap gap-1.5">
            {PLUGIN_PARTS.map((part) => (
              <span key={part} className="border border-line bg-paper px-2 py-1 text-xs text-ink">
                {part}
              </span>
            ))}
          </div>
        </Layer>

        <Step>installs it into every agent, in each one&apos;s own format</Step>

        <Layer label="Your agents">
          <div className={quad}>
            {harnessMarks.map((harness) => (
              <div
                key={harness.name}
                className="flex items-center justify-center gap-2 border border-line bg-paper px-2 py-3 text-[13px] font-semibold text-ink"
              >
                <HarnessMark icon={harness.icon} className="size-4 shrink-0" />
                {harness.name}
              </div>
            ))}
          </div>
        </Layer>

        <Step>gives each one a worktree on its own branch</Step>

        <Layer label="Worktrees">
          <div className={quad}>
            {BRANCHES.map((branch) => (
              <div key={branch} className="border border-dashed border-line px-2 py-2 text-center text-xs text-ink">
                {branch}
              </div>
            ))}
          </div>
        </Layer>

        <Step>lands the work: rebase, your checks, pull request</Step>

        <Layer label="Your repo">
          <div className="border border-line bg-paper px-3 py-2.5 text-center text-[13px] font-semibold text-ink">
            main
          </div>
        </Layer>
      </div>
    </figure>
  );
}
