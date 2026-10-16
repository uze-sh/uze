'use client';

import { useEffect, useRef, useState } from 'react';

// The hero, recorded rather than drawn: the real binary, in a sandbox, from
// .demo/landing/demo.yml. A drawing of a terminal read as a promise; this is
// the thing itself. Each chapter starts where the take reaches that beat, in
// seconds of the rendered video, so re-recording means re-reading these off
// the new cut (`tui-record run` prints where each beat landed).
const CHAPTERS: { label: string; at: number }[] = [
  { label: 'open the workspace', at: 0 },
  { label: 'add a plugin', at: 0 },
  { label: 'agents side by side', at: 0 },
  { label: 'review and deliver', at: 0 },
];

export function HeroRecording() {
  const video = useRef<HTMLVideoElement>(null);
  const [current, setCurrent] = useState(0);
  const [still, setStill] = useState(false);

  // Reduced motion gets the poster and nothing that moves on its own; a
  // chapter picked there plays from it, because that is a reader asking.
  useEffect(() => {
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    setStill(reduced);
    if (!reduced) void video.current?.play().catch(() => {});
  }, []);

  const follow = () => {
    const t = video.current?.currentTime ?? 0;
    setCurrent(CHAPTERS.reduce((found, chapter, index) => (t >= chapter.at ? index : found), 0));
  };

  const jump = (index: number) => {
    const el = video.current;
    if (!el) return;
    el.currentTime = CHAPTERS[index].at;
    setCurrent(index);
    void el.play().catch(() => {});
  };

  return (
    <div className="border-y border-line">
      {/* The chapters, where a console keeps its tabs: the one playing is
          filled, and any of them can be picked. */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-line py-2 font-mono text-xs sm:justify-between">
        {CHAPTERS.map((chapter, index) => {
          const active = index === current;
          return (
            <button
              key={chapter.label}
              type="button"
              onClick={() => jump(index)}
              aria-current={active ? 'step' : undefined}
              className={`inline-flex items-center gap-2 py-1 transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
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

      {/* The recording's own ground is the TUI's, a shade off the frame's
          black: the frame is painted in it too, so the edge never shows. */}
      <div className="py-3" style={{ background: '#0a0c0d' }}>
        <video
          ref={video}
          muted
          loop
          playsInline
          preload={still ? 'none' : 'auto'}
          poster="/uze-landing-poster.png"
          onTimeUpdate={follow}
          aria-label="A terminal session recorded with the real uze: uze workspace opens; in its shell, uze market add hiukky/ai adds a marketplace, uze git@ai installs the git plugin and uze inspect git shows what Claude Code, Codex, OpenCode and Antigravity each received; agents on Claude Code, Codex and OpenCode then work side by side, each in a worktree of its own; spec, arch and code open on the first, which commits through the plugin skill and is delivered."
          className="block h-auto w-full"
        >
          <source src="/uze-landing.webm" type="video/webm" />
          <source src="/uze-landing.mp4" type="video/mp4" />
        </video>
      </div>
    </div>
  );
}
