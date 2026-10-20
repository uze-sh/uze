'use client';

import { useEffect, useState } from 'react';

export function InstallCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
    } catch {
      // Clipboard access can be refused (insecure origin, denied permission).
      // The command is selectable text either way, so there is nothing to
      // recover from and nothing worth interrupting the reader about.
    }
  }

  return (
    <div className="glass flex items-center rounded-lg font-mono text-[13.5px]">
      {/* Wraps rather than scrolls: a horizontal scrollbar under a command
          is a thing to fight, and the copy button already removes the reason
          to select it by hand. */}
      <code className="flex-1 px-4 py-3.5 leading-6 text-ink whitespace-pre-wrap break-words">
        <span className="text-muted select-none">$ </span>
        {command}
      </code>
      <button
        type="button"
        onClick={copy}
        aria-label={copied ? 'Copied' : 'Copy the command'}
        className="me-2 shrink-0 rounded-md p-2 text-muted transition-colors hover:bg-ink/5 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-accent"
      >
        {copied ? <CheckIcon /> : <CopyIcon />}
      </button>
    </div>
  );
}

function CopyIcon() {
  return (
    <svg aria-hidden width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect x="9" y="9" width="12" height="12" rx="2" />
      <path d="M5 15V5a2 2 0 0 1 2-2h10" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg
      aria-hidden
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill="none"
      stroke="var(--color-success)"
      strokeWidth="2.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}
