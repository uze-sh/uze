'use client';

import { type MutableRefObject, useEffect, useRef } from 'react';
import 'asciinema-player/dist/bundle/asciinema-player.css';

// A recording played as text: the take's own cut cast (.demo/*/demo.yml
// publishes it), drawn by asciinema-player in the page's own type, so it is
// as sharp as the page at any size and any zoom. A GIF of the same take
// carried a 256-colour palette and the renderer's blur.

export type CastHandle = {
  play: () => Promise<void>;
  pause: () => void;
  seek: (to: number) => Promise<void>;
  getCurrentTime: () => number;
  dispose: () => void;
};

// The pace every spec records at: what reads as a person typing.
const SPEED = 1.2;

export function CastPlayer({
  src,
  posterAt,
  label,
  handle,
  startAt,
}: {
  src: string;
  /** The second of the cast shown before it plays, and to reduced motion. */
  posterAt: number;
  label: string;
  handle?: MutableRefObject<CastHandle | null>;
  /** Where to begin playing, for a player that takes over from another. */
  startAt?: number;
}) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    let player: CastHandle | null = null;
    let seen: IntersectionObserver | undefined;
    let disposed = false;
    // The player measures a cell in the font it is given, so the font is
    // loaded first or every row is laid out at the fallback's width and
    // overflows. It plays only while on screen: a docs page can hold several,
    // and one running out of sight is work nobody watches.
    void Promise.all([
      import('asciinema-player'),
      document.fonts.load("16px 'Uze Terminal'"),
      document.fonts.load("bold 16px 'Uze Terminal'"),
    ]).then(([{ create }]) => {
      if (disposed) return;
      player = create(src, el, {
        loop: true,
        speed: SPEED,
        fit: 'width',
        controls: false,
        poster: `npt:${posterAt}`,
        ...(startAt ? { startAt } : {}),
        theme: 'uze',
        terminalFontFamily: "'Uze Terminal', 'JetBrains Mono', monospace",
        terminalLineHeight: 1.22,
      }) as CastHandle;
      if (handle) handle.current = player;
      if (still) return;
      seen = new IntersectionObserver(
        ([entry]) => void (entry.isIntersecting ? player?.play().catch(() => {}) : player?.pause()),
        { threshold: 0.35 },
      );
      seen.observe(el);
    });
    return () => {
      disposed = true;
      seen?.disconnect();
      player?.dispose();
      if (handle) handle.current = null;
    };
  }, [src, posterAt, handle, startAt]);

  return <div ref={host} className="cast-player" role="img" aria-label={label} />;
}
