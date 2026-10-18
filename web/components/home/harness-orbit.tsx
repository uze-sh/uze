'use client';

import { useEffect, useState, type ReactNode } from 'react';
import { Brain, DraftingCompass, FileText, GitCompare, GitFork, Plug, Webhook, WandSparkles } from 'lucide-react';
import { HarnessMark, harnessMarks, homepageOf } from '@/components/harness-marks';

// The agents uze serves, set around the claim as tiles of glass: the claim is
// what stays, and they are what comes and goes around it. Placed off a grid
// on purpose, each at its own tilt and depth, so they read as objects in a
// space rather than as a row of logos. Only beside the full claim: on a
// phone they would sit on the words.
// Offsets from the claim's centre, in pixels: the claim is centred and sized
// by its words, so a share of its box lands on the words themselves.
const PLACEMENT: Record<string, { x: number; y: number; size: number; tilt: number; delay: string }> = {
  'Claude Code': { x: -540, y: -120, size: 76, tilt: -9, delay: '0s' },
  OpenCode: { x: -470, y: 130, size: 60, tilt: 7, delay: '-2.4s' },
  Codex: { x: 545, y: -95, size: 72, tilt: 8, delay: '-1.2s' },
  Antigravity: { x: 470, y: 150, size: 62, tilt: -6, delay: '-3.6s' },
};

type Face = { name: string; href: string; mark: ReactNode; labelled: boolean };

const capability = (name: string, href: string, Icon: typeof Plug): Face => ({
  name,
  href,
  mark: <Icon className="size-[40%]" strokeWidth={1.6} aria-hidden />,
  labelled: true,
});

// The tiles stay and what is in them changes: first who uze serves, then
// what it brings each of them. A brand names itself; a capability's glyph
// does not, so only those carry a caption. One row per phase, in the order
// of `harnessMarks`, so a tile keeps its place across every phase.
const PHASES: Face[][] = [
  harnessMarks.map((harness) => ({
    name: harness.name,
    href: homepageOf(harness.name) ?? '/docs/reference/harnesses',
    mark: <HarnessMark icon={harness.icon} className="size-[42%]" />,
    labelled: false,
  })),
  [
    capability('Skills', '/docs/reference/skills', WandSparkles),
    capability('MCP', '/docs/reference/plugin-format#mcp-servers', Plug),
    capability('Hooks', '/docs/reference/plugin-format#hooks', Webhook),
    capability('AGENTS.md', '/docs/plugins/context', FileText),
  ],
  [
    capability('Worktrees', '/docs/workspace/agents#where-an-agent-works', GitFork),
    capability('Memory', '/docs/workspace/agents#continuing-in-another-harness', Brain),
    capability('Specs & diagrams', '/docs/workspace#extensions', DraftingCompass),
    capability('Changes', '/docs/workspace/code', GitCompare),
  ],
];

const HOLD_MS = 4800;
const STAGGER_MS = 140;

export function HarnessOrbit() {
  const [phase, setPhase] = useState(0);
  const [held, setHeld] = useState(false);

  // Reduced motion keeps the first phase: a change nobody asked for is
  // motion too, however gently it fades.
  useEffect(() => {
    if (held || window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const timer = window.setInterval(() => {
      if (!document.hidden) setPhase((current) => (current + 1) % PHASES.length);
    }, HOLD_MS);
    return () => window.clearInterval(timer);
  }, [held]);

  return (
    <ul
      className="pointer-events-none absolute inset-0 hidden xl:block"
      aria-label="Works with"
      onPointerOver={() => setHeld(true)}
      onPointerOut={() => setHeld(false)}
      onFocus={() => setHeld(true)}
      onBlur={() => setHeld(false)}
    >
      {harnessMarks.map((harness, slot) => {
        const place = PLACEMENT[harness.name];
        const face = PHASES[phase][slot];
        const external = face.href.startsWith('http');
        return (
          <li
            key={harness.name}
            className="harness-float absolute"
            style={{
              left: `calc(50% + ${place.x}px)`,
              top: `calc(50% + ${place.y}px)`,
              animationDelay: place.delay,
              ['--tilt' as string]: `${place.tilt}deg`,
            }}
          >
            <a
              href={face.href}
              {...(external ? { target: '_blank', rel: 'noreferrer noopener' } : {})}
              title={face.name}
              className="glass-tile pointer-events-auto relative flex -translate-x-1/2 -translate-y-1/2 items-center justify-center text-ink focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-accent"
              style={{ width: place.size, height: place.size }}
            >
              {PHASES.map((faces, index) => (
                <span
                  key={faces[slot].name}
                  className="orbit-face absolute inset-0 flex items-center justify-center"
                  data-active={index === phase}
                  style={{ transitionDelay: `${slot * STAGGER_MS}ms` }}
                  aria-hidden
                >
                  {faces[slot].mark}
                </span>
              ))}
              <span className="sr-only">{face.name}</span>
            </a>
            {PHASES.map((faces, index) =>
              faces[slot].labelled ? (
                <span
                  key={faces[slot].name}
                  className="orbit-face absolute left-0 -translate-x-1/2 whitespace-nowrap font-mono text-[11px] font-semibold tracking-tight text-muted"
                  data-active={index === phase}
                  style={{ top: place.size / 2 + 12, transitionDelay: `${slot * STAGGER_MS}ms` }}
                  aria-hidden
                >
                  {faces[slot].name}
                </span>
              ) : null,
            )}
          </li>
        );
      })}
    </ul>
  );
}
