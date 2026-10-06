'use client';

import { type ReactNode, useEffect, useRef } from 'react';

// How far the page scrolls while the claim hands the screen to the session,
// as a share of the viewport: long enough to read as a transition, short
// enough that nobody scrolls through dead space to reach the questions.
const HANDOFF = 0.6;

/**
 * The first screen, split in two: the claim in the top half and the session
 * peeking from the bottom one. Scrolling does not move the page past it; it
 * pins the screen, fades the claim out and lifts the session to its centre,
 * and only then lets the sections below arrive.
 *
 * Progress is written as CSS variables on the track rather than as React
 * state, so a scroll never re-renders the session that is playing inside it.
 * Below `md` the session is the stacked summary, and the hero is the plain
 * screen it always was.
 */
export function HeroStage({ hero, session }: { hero: ReactNode; session: ReactNode }) {
  const track = useRef<HTMLDivElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const frame = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const trackEl = track.current;
    const stageEl = stage.current;
    const frameEl = frame.current;
    if (!trackEl || !stageEl || !frameEl) return;

    const wide = window.matchMedia('(min-width: 768px)');
    const still = window.matchMedia('(prefers-reduced-motion: reduce)');
    let raf = 0;
    let top = 0;

    const measure = () => {
      const nav = document.getElementById('nd-nav');
      top = nav ? Math.max(0, nav.getBoundingClientRect().bottom) : 0;
      const stageHeight = window.innerHeight - top;
      // Centred once it has arrived, but never pushed under the header on a
      // screen too short to centre it.
      const rest = Math.max(24, (stageHeight - frameEl.offsetHeight) / 2);
      trackEl.style.setProperty('--stage-top', `${top}px`);
      trackEl.style.setProperty('--stage-h', `${stageHeight}px`);
      trackEl.style.setProperty('--peek', `${stageHeight / 2}px`);
      trackEl.style.setProperty('--rest', `${rest}px`);
      trackEl.style.setProperty('--handoff', `${window.innerHeight * HANDOFF}px`);
    };

    const update = () => {
      raf = 0;
      if (!wide.matches) return;
      const travel = trackEl.offsetHeight - stageEl.offsetHeight;
      // The stage pins under the header, so the hand-off starts when the
      // track reaches it, not when it reaches the top of the window.
      const raw = travel > 0 ? (top - trackEl.getBoundingClientRect().top) / travel : 0;
      const clamped = Math.min(1, Math.max(0, raw));
      // A reader who asked for no motion still gets both states, without
      // the frames between them.
      const p = still.matches ? (clamped < 0.5 ? 0 : 1) : clamped;
      trackEl.style.setProperty('--p', p.toFixed(4));
    };

    const schedule = () => {
      if (!raf) raf = requestAnimationFrame(update);
    };
    const resize = () => {
      measure();
      schedule();
    };

    resize();
    window.addEventListener('scroll', schedule, { passive: true });
    window.addEventListener('resize', resize);
    wide.addEventListener('change', resize);
    // The banner can be dismissed, which moves the header and so the stage.
    const observer = new ResizeObserver(resize);
    const nav = document.getElementById('nd-nav');
    if (nav) observer.observe(nav);
    observer.observe(frameEl);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', resize);
      wide.removeEventListener('change', resize);
      observer.disconnect();
    };
  }, []);

  return (
    <div
      ref={track}
      className="[--p:0] md:h-[calc(var(--stage-h,100svh)+var(--handoff,60svh))]"
    >
      <div
        ref={stage}
        className="md:sticky md:top-[var(--stage-top,0px)] md:h-[var(--stage-h,100svh)] md:overflow-hidden"
      >
        <div className="md:absolute md:inset-x-0 md:top-0 md:flex md:h-1/2 md:items-center md:justify-center md:opacity-[calc(1-var(--p)*1.8)] md:[filter:blur(calc(var(--p)*6px))] md:[transform:translateY(calc(var(--p)*-4rem))_scale(calc(1-var(--p)*0.04))]">
          {hero}
        </div>
        <div
          ref={frame}
          className="md:absolute md:inset-x-0 md:top-0 md:[transform:translateY(calc((1-var(--p))*var(--peek,50svh)+var(--p)*var(--rest,24px)))_scale(calc(0.94+var(--p)*0.06))] md:origin-top"
        >
          {session}
        </div>
      </div>
    </div>
  );
}
