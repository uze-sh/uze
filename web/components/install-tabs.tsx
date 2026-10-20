'use client';

import { useState, type ReactNode } from 'react';
import { InstallCommand } from '@/components/install-command';

type Platform = 'unix' | 'windows';

const platforms: { id: Platform; label: string; command: string; note: string }[] = [
  {
    id: 'unix',
    label: 'macOS · Linux',
    command: 'curl -fsSL https://uze.sh/i | sh',
    note: 'x86_64 or aarch64, signature and checksum verified.',
  },
  {
    id: 'windows',
    label: 'Windows',
    command: 'irm https://uze.sh/i | iex',
    note: 'Windows 10 22H2 or 11, x64 or ARM64, signature and checksum verified.',
  },
];

// macOS and Linux is selected for everyone, Windows visitors included: the
// tab a reader lands on is the same on every machine, and Windows is one
// click away.
export function InstallTabs({ action }: { action?: ReactNode }) {
  const [active, setActive] = useState<Platform>('unix');

  const current = platforms.find((platform) => platform.id === active) ?? platforms[0];

  return (
    <div className="text-left">
      <div
        role="tablist"
        aria-label="Install on"
        className="mb-2.5 inline-flex rounded-md border border-line p-0.5 font-mono text-[11.5px]"
      >
        {platforms.map((platform) => {
          const selected = platform.id === active;
          return (
            <button
              key={platform.id}
              type="button"
              role="tab"
              aria-selected={selected}
              onClick={() => setActive(platform.id)}
              className={`rounded-[5px] px-2.5 py-1 transition-colors focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent ${
                selected ? 'bg-ink text-paper' : 'text-muted hover:text-ink'
              }`}
            >
              {platform.label}
            </button>
          );
        })}
      </div>
      <div role="tabpanel">
        <InstallCommand command={current.command} />
      </div>
      <div className="mt-2.5 flex items-baseline gap-x-4">
        {/* Every platform's note in the one cell, the others invisible: the
            cell is always the size of the longest, so switching platform
            moves nothing beside it. */}
        <div className="grid min-w-0 flex-1 px-0.5 text-xs leading-5 text-muted">
          {platforms.map((platform) => (
            <p
              key={platform.id}
              aria-hidden={platform.id !== active}
              className={`col-start-1 row-start-1 ${platform.id === active ? '' : 'invisible'}`}
            >
              {platform.note}
            </p>
          ))}
        </div>
        {action ? <div className="shrink-0">{action}</div> : null}
      </div>
    </div>
  );
}
