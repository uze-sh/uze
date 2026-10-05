import type { ReactNode } from 'react';
import Link from 'next/link';
import { HarnessMark, harnessMarks } from '@/components/harness-marks';

// The session, said in its three commands, for a screen too narrow to play
// it: four agent lanes and the workspace's two panes need a tablet's width,
// and squeezed under that they read as a broken terminal rather than as a
// demonstration. What survives is the shape of using uze, which is the point.
// Numbered because it is a sequence: each step needs the one before it.
export function SessionSummary() {
  return (
    <div>
      <ol className="space-y-9">
        <Step number={1} title="Add a marketplace" command="uze market add hiukky/ai" />
        <Step number={2} title="Install a plugin into every agent" command="uze git@ai">
          <ul className="mt-4 grid grid-cols-2 gap-x-4 gap-y-3">
            {harnessMarks.map((harness) => (
              <li key={harness.name} className="flex items-center gap-2 font-mono text-[13px] text-ink">
                <span className="w-3 shrink-0 text-success" aria-label="installed">
                  ✓
                </span>
                <HarnessMark icon={harness.icon} className="size-4 shrink-0" />
                <span className="truncate">{harness.name}</span>
              </li>
            ))}
          </ul>
        </Step>
        <Step number={3} title="Run them side by side" command="uze workspace">
          <p className="mt-3 text-sm leading-relaxed text-muted">
            Each agent on a branch of its own, in its own worktree.
          </p>
        </Step>
      </ol>
      <p className="mt-10 text-sm text-muted">
        <Link
          href="/docs/reference/harnesses"
          className="text-ink underline decoration-line underline-offset-4 transition-colors hover:decoration-ink"
        >
          What each agent receives
        </Link>
        , capability by capability.
      </p>
    </div>
  );
}

function Step({
  number,
  title,
  command,
  children,
}: {
  number: number;
  title: string;
  command: string;
  children?: ReactNode;
}) {
  return (
    <li className="grid grid-cols-[1.75rem_minmax(0,1fr)] gap-x-3">
      <span className="pt-0.5 font-mono text-sm text-muted" aria-hidden>
        {number}
      </span>
      <div>
        <p className="text-[15px] font-semibold text-ink">{title}</p>
        <p className="mt-2.5 border border-line bg-surface/40 px-3 py-2.5 font-mono text-[13px] break-words text-ink">
          <span className="text-muted select-none">$ </span>
          {command}
        </p>
        {children}
      </div>
    </li>
  );
}
