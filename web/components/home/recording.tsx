'use client';

import { useEffect, useRef, useState } from 'react';
import { CastHandle, CastPlayer } from '@/components/cast-player';

// The hero, recorded rather than drawn: the real binary, in a sandbox, from
// .demo/landing/demo.yml. Each chapter starts where the take reaches that
// beat, in seconds of the cast: a new take means reading these off the new
// cut.
const CAST = '/casts/landing.cast';
const CHAPTERS: { label: string; at: number }[] = [
  { label: 'open the workspace', at: 0 },
  { label: 'add a plugin', at: 4 },
  { label: 'agents side by side', at: 16.7 },
  { label: 'review and deliver', at: 41.4 },
  { label: 'pick up anywhere', at: 58 },
];
// Three agents named, each in its own worktree, and Claude's commit.
const POSTER_AT = 39.7;
const FRAME_WIDTH = 'min(calc((100svh - 16rem) * 1.89), calc(100vw - 4rem), 1600px)';

export function HeroRecording() {
  const player = useRef<CastHandle | null>(null);
  const [current, setCurrent] = useState(0);

  useEffect(() => {
    const follow = setInterval(() => {
      const t = player.current?.getCurrentTime() ?? 0;
      setCurrent(CHAPTERS.reduce((found, chapter, index) => (t >= chapter.at ? index : found), 0));
    }, 250);
    return () => clearInterval(follow);
  }, []);

  const jump = (index: number) => {
    const p = player.current;
    if (!p) return;
    setCurrent(index);
    void p.seek(CHAPTERS[index].at).then(() => p.play());
  };

  return (
    <div>
      {/* Wider than the page's column, in a bezel: the recording is the
          proof, so it gets the room a screenshot of an app would. Its width
          follows the screen's height, so the pinned stage never cuts it: the
          terminal is about 1.89:1, and 16rem is the banner, the header,
          the stage's margin, the bezel, the title bar and the chapters
          under it, with room for the frame's shadow inside the stage. Centred on the column it breaks out of. */}
      <figure
        className="hero-frame dark relative left-1/2 -translate-x-1/2 rounded-[22px] bg-[#16181a] p-2 text-ink"
        style={{ width: FRAME_WIDTH }}
      >
        <div className="overflow-hidden rounded-[14px] bg-[#0a0c0d] ring-1 ring-white/[0.06]">
          <div className="relative flex h-9 items-center border-b border-white/[0.06] px-4">
            <div className="flex gap-2" aria-hidden>
              <span className="size-3 rounded-full bg-[#ff5f57]" />
              <span className="size-3 rounded-full bg-[#febc2e]" />
              <span className="size-3 rounded-full bg-[#28c840]" />
            </div>
            <figcaption className="absolute inset-x-0 text-center font-mono text-xs text-muted">
              ~/demo-api · uze
            </figcaption>
          </div>
          <CastPlayer
            src={CAST}
            posterAt={POSTER_AT}
            handle={player}
            label="A terminal session recorded with the real uze: uze workspace opens; in its shell, uze market add hiukky/ai adds a marketplace, uze git@ai installs the git plugin and uze inspect git shows what Claude Code, Codex, OpenCode and Antigravity each received; agents on Claude Code, Codex and OpenCode then work side by side, each in a worktree of its own; spec, arch and code open on one, while Claude commits through the plugin skill and is delivered to main; the OpenCode agent then continues in Claude Code, is closed, found in Recent and resumed, and the management screens are shown."
          />
        </div>
      </figure>

      {/* The chapters, under the window rather than in it, where nobody
          reads them as part of the app: the one playing is filled, and any
          can be picked. As wide as the window, so it sits framed by it. */}
      <div
        className="relative left-1/2 mt-6 flex -translate-x-1/2 flex-wrap items-center justify-between gap-x-4 gap-y-1 px-3 font-mono text-xs"
        style={{ width: FRAME_WIDTH }}
      >
        {CHAPTERS.map((chapter, index) => {
          const active = index === current;
          return (
            <button
              key={chapter.label}
              type="button"
              onClick={() => jump(index)}
              aria-current={active ? 'step' : undefined}
              className={`inline-flex items-center gap-2 py-1 whitespace-nowrap transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
                active ? 'text-ink' : 'text-muted hover:text-ink'
              }`}
            >
              <span
                className="inline-flex size-5 items-center justify-center text-[11px] font-semibold"
                style={
                  active
                    ? { background: 'var(--color-accent)', color: 'var(--color-paper)' }
                    : { boxShadow: 'inset 0 0 0 1px var(--color-line)' }
                }
              >
                {index + 1}
              </span>
              {chapter.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}
