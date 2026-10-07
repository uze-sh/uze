'use client';

import { type ReactNode, useEffect, useRef } from 'react';

// How far the page scrolls while the stage stays pinned, as a share of the
// viewport: long enough to read as a transition, short enough that nobody
// scrolls through dead space to reach the questions.
const HANDOFF = 0.6;

// The pace the hand-off keeps when the scroll driving it is slower, or has
// stopped: the whole of it in about this long, so a gentle scroll still
// carries it through in one smooth movement and never leaves it halfway.
const CRUISE_MS = 550;

// How quickly the hand-off gathers its cruising pace from rest, so a first
// notch does not start it with a jolt.
const RAMP_MS = 140;

// How long the hand-off takes to settle into either end once it is close: it
// slows in rather than stopping dead.
const LANDING_MS = 90;

// How quickly the speed a scroll gave the hand-off fades, per millisecond.
const SPEED_DECAY = 0.992;

// How far the page must move against the way it was going before it counts
// as a turn: above the jitter a trackpad sends at the end of a swipe.
const TURN_PX = 8;

/**
 * The first screen, split in two: the claim in the top half and the session
 * peeking from the bottom one. Scrolling does not move the page past it at
 * once; it pins the screen while the claim fades out and the session lifts
 * to its centre, and only then lets the sections below arrive.
 *
 * The hand-off follows the scroll, at the reader's speed: a flick runs it
 * fast, a slow scroll runs it slowly. It is mapped onto what is left of the
 * pinned screen in the direction the reader is going, so it is always whole
 * by the time the page moves on, whichever way and from wherever it started.
 * It never waits on the scroll either: once the reader heads into it, it
 * keeps moving towards that end at the scroll's speed or a gentle cruising
 * pace, whichever is faster, and slows into the end. The page itself is
 * never moved for the reader, and nothing past the pinned screen is touched.
 *
 * Progress is written as a CSS variable on the track rather than as React
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
    // Where the pinned screen starts and ends, in page coordinates.
    let start = 0;
    let end = 0;
    let shown = 0;
    let lastY = window.scrollY;
    let lastAt = performance.now();
    let heading = 0;
    let turned = 0;
    // Which end the hand-off is moving to, and how fast, in progress per ms.
    let goal = 0;
    let pace = 0;
    let scrollSpeed = 0;
    let running = 0;
    let lastFrame = 0;

    const measure = () => {
      const nav = document.getElementById('nd-nav');
      const top = nav ? Math.max(0, nav.getBoundingClientRect().bottom) : 0;
      const stageHeight = window.innerHeight - top;
      // Centred once it has arrived, but never pushed under the header on a
      // screen too short to centre it.
      // Whole pixels: a frame resting on a half pixel draws every filled row
      // with a seam along its edge.
      const rest = Math.max(24, Math.round((stageHeight - frameEl.offsetHeight) / 2));
      trackEl.style.setProperty('--stage-top', `${top}px`);
      trackEl.style.setProperty('--stage-h', `${stageHeight}px`);
      trackEl.style.setProperty('--peek', `${Math.round(stageHeight / 2)}px`);
      trackEl.style.setProperty('--rest', `${rest}px`);
      trackEl.style.setProperty('--handoff', `${window.innerHeight * HANDOFF}px`);
      // The stage pins under the header, so the hand-off starts when the
      // track reaches it, not when it reaches the top of the window.
      start = window.scrollY + trackEl.getBoundingClientRect().top - top;
      end = start + Math.max(0, trackEl.offsetHeight - stageEl.offsetHeight);
    };

    const paint = (p: number) => {
      shown = Math.min(1, Math.max(0, p));
      trackEl.style.setProperty('--p', shown.toFixed(4));
      // Promoted to layers of their own only while they move, so a frame
      // composites rather than repaints. At rest the frame also drops its
      // transform: a layer scaled while it moved can keep the raster it got
      // mid-way, and every hairline in the session then reads as a
      // staircase until something repaints it.
      trackEl.toggleAttribute('data-moving', shown > 0 && shown < 1);
      trackEl.toggleAttribute('data-rest', shown === 1);
    };

    const cruise = 1 / CRUISE_MS;

    // Once the hand-off is whole, what is left of the pinned screen that way
    // would be scrolled through with nothing changing on it, and the next
    // scroll would seem swallowed. Every position along the pinned screen
    // draws the same frame, so the page is moved to its end unseen.
    const skipPinnedRest = () => {
      const y = window.scrollY;
      const to = goal === 1 && y > start && y < end ? end : goal === 0 && y > start && y < end ? start : y;
      if (to === y) return;
      lastY = to;
      window.scrollTo({ top: to, behavior: 'instant' });
    };

    const frameStep = (now: number) => {
      const dt = Math.min(50, now - lastFrame);
      lastFrame = now;
      scrollSpeed *= SPEED_DECAY ** dt;
      const left = Math.abs(goal - shown);
      if (left < 0.003) {
        paint(goal);
        pace = 0;
        running = 0;
        skipPinnedRest();
        return;
      }
      // The ramp only eases the start of the cruising pace; a scroll faster
      // than it is followed at once, or the hand-off would trail a flick.
      const ramped = Math.min(cruise, pace + (cruise / RAMP_MS) * dt);
      pace = Math.min(Math.max(scrollSpeed, ramped), left / LANDING_MS);
      paint(shown + Math.sign(goal - shown) * pace * dt);
      running = requestAnimationFrame(frameStep);
    };

    const head = (to: number) => {
      if (to !== goal) pace = 0;
      goal = to;
      if (still.matches) {
        paint(to);
        return;
      }
      if (!running) {
        lastFrame = performance.now();
        running = requestAnimationFrame(frameStep);
      }
    };

    const onScroll = () => {
      if (!wide.matches) return;
      const y = window.scrollY;
      const now = performance.now();
      const moved = y - lastY;
      const elapsed = Math.max(1, now - lastAt);
      const before = Math.min(end, Math.max(start, lastY));
      const after = Math.min(end, Math.max(start, y));
      lastY = y;
      lastAt = now;
      if (moved === 0) return;

      if (Math.sign(moved) === heading) {
        turned = 0;
      } else {
        turned += Math.abs(moved);
        if (turned >= TURN_PX || heading === 0) {
          heading = Math.sign(moved);
          turned = 0;
        }
      }

      // Nothing to do outside the pinned screen: the sections below scroll
      // as any page does.
      if (after === before) return;
      const travel = Math.max(1, end - start);
      // A scroll faster than the cruising pace drives the hand-off at its
      // own speed: the pinned screen's length end to end.
      if (elapsed < 100) scrollSpeed = Math.max(scrollSpeed, Math.abs(after - before) / travel / elapsed);
      if (after > before) {
        // Never behind what is left of the pinned screen going down, so the
        // session is seated by the time the page moves on.
        const due = 1 - (1 - shown) * ((end - after) / Math.max(1, end - before));
        if (due > shown) paint(due);
        if (heading > 0) head(1);
      } else if (heading < 0) {
        head(0);
      }
    };

    const resize = () => {
      measure();
      if (!wide.matches) return;
      const y = window.scrollY;
      // Arriving on a page already scrolled, the stage shows where it is.
      if (y >= end) paint(1);
      else if (y <= start && shown < 1) paint(0);
    };

    resize();
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', resize);
    wide.addEventListener('change', resize);
    // The banner can be dismissed, which moves the header and so the stage.
    const observer = new ResizeObserver(resize);
    const nav = document.getElementById('nd-nav');
    if (nav) observer.observe(nav);
    observer.observe(frameEl);
    return () => {
      cancelAnimationFrame(running);
      window.removeEventListener('scroll', onScroll);
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
        <div className="md:absolute md:inset-x-0 md:top-0 md:flex md:h-1/2 md:items-center md:justify-center md:opacity-[calc(1-var(--p)*1.8)] md:[filter:blur(calc(var(--p)*6px))] md:[transform:translateY(calc(var(--p)*-4rem))_scale(calc(1-var(--p)*0.04))] md:in-data-moving:will-change-[opacity,filter,transform]">
          {hero}
        </div>
        <div
          ref={frame}
          className="md:absolute md:inset-x-0 md:top-0 md:[transform:translateY(calc((1-var(--p))*var(--peek,50svh)+var(--p)*var(--rest,24px)))_scale(calc(0.94+var(--p)*0.06))] md:origin-top md:in-data-rest:top-[var(--rest,24px)] md:in-data-rest:[transform:none] md:in-data-moving:will-change-transform"
        >
          {session}
        </div>
      </div>
    </div>
  );
}
