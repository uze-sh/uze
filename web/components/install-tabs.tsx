'use client';

import { useState } from 'react';
import { InstallCommand } from '@/components/install-command';

type Platform = 'unix' | 'windows';

const platforms: { id: Platform; label: string; command: string; note: string }[] = [
  {
    id: 'unix',
    label: 'macOS · Linux',
    command: 'curl -fsSL https://uze.sh/i | sh',
    note: 'x86_64 or aarch64. Checksum verified, installs into ~/.local/bin.',
  },
  {
    id: 'windows',
    label: 'Windows',
    command: 'irm https://uze.sh/i | iex',
    note: 'Windows 10 22H2 or 11, x64 or Arm. Needs Git for Windows.',
  },
];

// macOS and Linux is selected for everyone, Windows visitors included: the
// tab a reader lands on is the same on every machine, and Windows is one
// click away.
export function InstallTabs() {
  const [active, setActive] = useState<Platform>('unix');

  const current = platforms.find((platform) => platform.id === active) ?? platforms[0];

  return (
    <div className="text-left">
      <div role="tablist" aria-label="Install on" className="flex gap-px font-mono text-[11.5px]">
        {platforms.map((platform) => {
          const selected = platform.id === active;
          return (
            <button
              key={platform.id}
              type="button"
              role="tab"
              aria-selected={selected}
              onClick={() => setActive(platform.id)}
              className={`border border-b-0 px-3 py-1.5 transition-colors ${
                selected ? 'border-line bg-surface/40 text-ink' : 'border-transparent text-muted hover:text-ink'
              }`}
            >
              {platform.label}
            </button>
          );
        })}
      </div>
      <div role="tabpanel">
        <InstallCommand command={current.command} />
        <p className="mt-2 text-xs text-muted">{current.note}</p>
      </div>
    </div>
  );
}
