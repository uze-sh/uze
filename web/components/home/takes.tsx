'use client';

import { useEffect, useRef, useState } from 'react';

// A run of cells sharing one style: its text and an index into `styles`.
export type Run = [text: string, style: number];
// A style as the terminal drew it: ink, ground and weight.
export type Style = [fg: string | null, bg: string | null, bold: 0 | 1];
export type Take = { label: string; rows: Run[][] };

const HOLD_MS = 3600;
// FiraCode Nerd Font Mono's own metrics, in its units: every cell is 1200
// wide, and a row is its ascent and descent, 1800 + 600, over an em of 1950.
const EM = 1950;
const ADVANCE = 1200;
const ASCENT = 1800;
const HEIGHT = 2400;
const INK = '#e6e4de';
const GROUND = '#0a0c0d';
const FACE = "'Uze Terminal'";

// Several moments of one behaviour, one at a time: a frame that tried to
// hold them all at once read as a wall of terminal. They take turns while
// the card is on screen, and a tab picks one; under reduced motion they
// wait to be picked.
export function Takes({ title, takes, styles, cols }: { title: string; takes: Take[]; styles: Style[]; cols: number }) {
  const [active, setActive] = useState(0);
  const [held, setHeld] = useState(false);
  const [visible, setVisible] = useState(false);
  const [still, setStill] = useState(true);
  const card = useRef<HTMLElement>(null);

  useEffect(() => {
    setStill(window.matchMedia('(prefers-reduced-motion: reduce)').matches);
    const observer = new IntersectionObserver(([entry]) => setVisible(entry.isIntersecting), { threshold: 0.5 });
    if (card.current) observer.observe(card.current);
    return () => observer.disconnect();
  }, []);

  const playing = visible && !held && !still && takes.length > 1;
  useEffect(() => {
    if (!playing) return;
    const timer = window.setTimeout(() => setActive((current) => (current + 1) % takes.length), HOLD_MS);
    return () => window.clearTimeout(timer);
  }, [playing, active, takes.length]);

  return (
    <figure
      ref={card}
      className="overflow-hidden rounded-xl bg-[#0a0c0d] shadow-[0_0_0_1px_#1d1f20,0_24px_60px_-24px_rgb(0_0_0/0.45)]"
      onPointerEnter={() => setHeld(true)}
      onPointerLeave={() => setHeld(false)}
      onFocus={() => setHeld(true)}
      onBlur={() => setHeld(false)}
      aria-label={title}
    >
      <div className="flex items-center gap-1.5 border-b border-[#161819] ps-4">
        {['#ff5f57', '#febc2e', '#28c840'].map((dot) => (
          <span key={dot} className="size-2.5 shrink-0 rounded-full" style={{ background: dot }} aria-hidden />
        ))}
        <span className="ms-3 min-w-0 flex-1 truncate font-mono text-[11.5px] text-[#6b7176]">{title}</span>
        {takes.length > 1 ? (
          <div className="flex shrink-0" role="tablist" aria-label={`${title}: moments`}>
            {takes.map((take, index) => (
              <button
                key={take.label}
                type="button"
                role="tab"
                aria-selected={index === active}
                onClick={() => setActive(index)}
                className="relative px-3 py-2.5 font-mono text-[11.5px] transition-colors focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-[#f2f0ea]"
                style={{ color: index === active ? '#f2f0ea' : '#6b7176' }}
              >
                {take.label}
                {index === active ? (
                  <span
                    key={`${active}-${playing}`}
                    className="take-progress absolute inset-x-3 bottom-0 h-px origin-left bg-[#f2f0ea]"
                    style={{ animationDuration: `${HOLD_MS}ms`, animationPlayState: playing ? 'running' : 'paused' }}
                    aria-hidden
                  />
                ) : null}
              </button>
            ))}
          </div>
        ) : null}
      </div>
      <div className="grid p-3">
        {takes.map((take, index) => (
          <Screen key={take.label} take={take} styles={styles} cols={cols} shown={index === active} />
        ))}
      </div>
    </figure>
  );
}

// A take is drawn the way a terminal draws itself: on a grid of whole
// device pixels, one glyph to a cell. Laid out as text, a row had the height
// the card's width gave it, fractional at most widths and zooms, and the
// edge between two rows' grounds landed mid-pixel and showed as a hairline;
// no rounding in CSS can reach a device pixel at a zoom like 125%.
function Screen({ take, styles, cols, shown }: { take: Take; styles: Style[]; cols: number; shown: boolean }) {
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const surface = canvas.current;
    const box = surface?.parentElement;
    if (!surface || !box) return;
    let cancelled = false;
    const draw = () => {
      if (cancelled) return;
      const ratio = window.devicePixelRatio || 1;
      // Only a row's height has to be whole: a cell's width is the card's,
      // and every edge across a row is rounded to the pixel it falls in, so
      // two neighbours share an edge rather than each antialiasing one.
      const cell = (box.clientWidth * ratio) / cols;
      const edge = (column: number) => Math.round(column * cell);
      const size = (cell * EM) / ADVANCE;
      const row = Math.round((size * HEIGHT) / EM);
      const baseline = Math.round((size * ASCENT) / EM + (row - (size * HEIGHT) / EM) / 2);
      surface.width = edge(cols);
      surface.height = row * take.rows.length;
      surface.style.width = `${surface.width / ratio}px`;
      surface.style.height = `${surface.height / ratio}px`;
      const context = surface.getContext('2d');
      if (!context) return;
      context.fillStyle = GROUND;
      context.fillRect(0, 0, surface.width, surface.height);
      context.textBaseline = 'alphabetic';
      take.rows.forEach((runs, line) => {
        const cells = runs.flatMap(([text, style]) => Array.from(text, (glyph) => ({ glyph, style: styles[style] })));
        // Grounds first, one rectangle per stretch of the same ground: runs
        // split where the ink changes, and the ground must not.
        for (let start = 0; start < cells.length; ) {
          const ground = cells[start].style[1];
          let end = start + 1;
          while (end < cells.length && cells[end].style[1] === ground) end += 1;
          if (ground) {
            context.fillStyle = ground;
            context.fillRect(edge(start), line * row, edge(end) - edge(start), row);
          }
          start = end;
        }
        cells.forEach(({ glyph, style: [fg, , bold] }, column) => {
          if (glyph === ' ') return;
          context.fillStyle = fg ?? INK;
          context.font = `${bold ? 700 : 400} ${size}px ${FACE}`;
          context.fillText(glyph, edge(column), line * row + baseline);
        });
      });
    };
    const faces = [400, 700].map((weight) => document.fonts.load(`${weight} 16px ${FACE}`, '\u{F01D8}'));
    Promise.all(faces).then(draw, draw);
    const observer = new ResizeObserver(draw);
    observer.observe(box);
    // A zoom that leaves the card's width in CSS pixels alone still changes
    // how many device pixels a cell is.
    window.addEventListener('resize', draw);
    return () => {
      cancelled = true;
      observer.disconnect();
      window.removeEventListener('resize', draw);
    };
  }, [take, styles, cols]);

  return (
    <div
      className="col-start-1 row-start-1 overflow-hidden transition-opacity duration-300"
      style={{ opacity: shown ? 1 : 0 }}
      aria-hidden
    >
      <canvas ref={canvas} className="block" />
    </div>
  );
}
