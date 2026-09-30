'use client';

import { useEffect, useState, type CSSProperties } from 'react';

// What the landing's two illustrations share: the palette, the stage they
// are drawn on and scaled from, the routes between panels, and the clock
// that plays them only while they are on screen.

// Every colour is one of the site's theme tokens, or a mix of them, so an
// illustration reads as part of the page in either theme rather than as a
// dark screenshot pasted onto a light one.
export const mix = (color: string, percent: number, base = 'transparent') =>
  `color-mix(in srgb, ${color} ${percent}%, ${base})`;
export const INK = 'var(--color-ink)';
export const PAPER = 'var(--color-paper)';
export const MUTED = 'var(--color-muted)';
export const G = 'var(--color-accent)';
export const AMB = 'var(--color-warn)';
export const RED = 'var(--color-danger)';
export const DIM = 'var(--color-line)';
export const LINE = 'var(--color-line)';
export const HOT = mix(G, 70);
export const FAINT = mix(MUTED, 55);
// Depth is opaque fills stepping from the page toward the ink, neutral in
// both themes: the surface token is tinted green in the dark one, and a whole
// panel of it reads as a cast. Green is kept for the marks: tags, checks,
// routes and the lit tab.
export const PANEL = mix(INK, 4, PAPER);
export const CARD = mix(INK, 7, PAPER);
export const CARD_ON = mix(INK, 12, PAPER);
export const SHEET = mix(INK, 9, PAPER);

export type Point = [number, number];

export const clamp = (x: number) => Math.max(0, Math.min(1, x));
export const ease = (x: number) => (x < 0.5 ? 2 * x * x : 1 - Math.pow(-2 * x + 2, 2) / 2);

export function roundedPath(points: Point[], radius = 14) {
  let d = `M${points[0][0]} ${points[0][1]}`;
  for (let i = 1; i < points.length - 1; i++) {
    const [px, py] = points[i - 1];
    const [cx, cy] = points[i];
    const [nx, ny] = points[i + 1];
    const before = Math.hypot(cx - px, cy - py);
    const after = Math.hypot(nx - cx, ny - cy);
    if (!before || !after) {
      d += ` L${cx} ${cy}`;
      continue;
    }
    const r = Math.min(radius, before / 2, after / 2);
    const ax = cx - ((cx - px) / before) * r;
    const ay = cy - ((cy - py) / before) * r;
    const bx = cx + ((nx - cx) / after) * r;
    const by = cy + ((ny - cy) / after) * r;
    d += ` L${ax} ${ay} Q${cx} ${cy} ${bx} ${by}`;
  }
  const [lx, ly] = points[points.length - 1];
  return `${d} L${lx} ${ly}`;
}

export function pointAlong(points: Point[], progress: number): Point {
  const lengths: number[] = [];
  let total = 0;
  for (let i = 1; i < points.length; i++) {
    const length = Math.hypot(points[i][0] - points[i - 1][0], points[i][1] - points[i - 1][1]);
    lengths.push(length);
    total += length;
  }
  let remaining = progress * total;
  for (let i = 1; i < points.length; i++) {
    if (remaining <= lengths[i - 1]) {
      const f = remaining / lengths[i - 1];
      return [
        points[i - 1][0] + (points[i][0] - points[i - 1][0]) * f,
        points[i - 1][1] + (points[i][1] - points[i - 1][1]) * f,
      ];
    }
    remaining -= lengths[i - 1];
  }
  return points[points.length - 1];
}

// Seconds into the loop, starting at `stillAt`: the frame a reader who asked
// for no motion keeps, and the one a page shows before it starts playing.
export function useClock(stageRef: React.RefObject<HTMLDivElement | null>, stillAt: number) {
  const [seconds, setSeconds] = useState(stillAt);
  useEffect(() => {
    const element = stageRef.current;
    if (!element) return;
    const still = window.matchMedia('(prefers-reduced-motion: reduce)');
    let raf = 0;
    let visible = false;
    let origin = 0;
    let elapsed = 0;
    const tick = (now: number) => {
      setSeconds(elapsed + (now - origin) / 1000);
      raf = requestAnimationFrame(tick);
    };
    const start = () => {
      if (raf || still.matches || !visible) return;
      origin = performance.now();
      raf = requestAnimationFrame(tick);
    };
    const stop = () => {
      if (!raf) return;
      cancelAnimationFrame(raf);
      raf = 0;
      elapsed += (performance.now() - origin) / 1000;
    };
    const onMotionChange = () => {
      if (still.matches) {
        stop();
        setSeconds(stillAt);
      } else start();
    };
    // Off screen, it stops: a landing page scrolled past has no reason to
    // repaint at sixty frames a second.
    const observer = new IntersectionObserver(([entry]) => {
      visible = entry.isIntersecting;
      if (visible) start();
      else stop();
    });
    observer.observe(element);
    still.addEventListener('change', onMotionChange);
    return () => {
      stop();
      observer.disconnect();
      still.removeEventListener('change', onMotionChange);
    };
  }, [stageRef, stillAt]);
  return seconds;
}

export function useColumnWidth(wrapRef: React.RefObject<HTMLDivElement | null>, initial: number) {
  const [width, setWidth] = useState(initial);
  useEffect(() => {
    const element = wrapRef.current;
    if (!element) return;
    const fit = () => setWidth(element.offsetWidth);
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    fit();
    return () => observer.disconnect();
  }, [wrapRef]);
  return width;
}

// Square and ruled, the way a terminal draws a box: a rounded, filled panel
// reads as a desktop app, and uze is not one.
export const panel: CSSProperties = {
  position: 'absolute',
  boxSizing: 'border-box',
  background: PANEL,
  border: `1px solid ${mix(INK, 16)}`,
  borderRadius: 0,
  overflow: 'hidden',
  display: 'flex',
  flexDirection: 'column',
};
export const nowrap: CSSProperties = { whiteSpace: 'nowrap' };

// Both illustrations: 1200×500 left to right where there is room, and the
// same three panels stacked top to bottom on a phone, where the wide stage
// would scale its text below reading size.
export const WIDE_STAGE = { width: 1200, height: 500 };
export const STACKED_STAGE = { width: 380, height: 1110 };
// Below this width the wide stage's 15px text would draw at under 9px.
export const STACK_BELOW = 700;
// On a wide screen it may draw larger than the design: at its own size it
// sat small in a 1440px window.
export const MAX_WIDE_SCALE = 1.08;

// A travelling dot on a route: something moving from one panel to another.
export function RouteDots({ dots }: { dots: Point[] }) {
  return (
    <>
      {dots.map(([x, y], i) => (
        <div
          key={i}
          style={{
            position: 'absolute',
            left: x,
            top: y,
            width: 10,
            height: 10,
            margin: '-5px 0 0 -5px',
            borderRadius: '50%',
            background: G,
            boxShadow: `0 0 14px ${G}`,
          }}
        />
      ))}
    </>
  );
}
