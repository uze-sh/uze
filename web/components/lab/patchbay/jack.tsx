// The page's one glyph: a jack, a ring with a pin in it. Lit means a cable is
// in it, which is the only thing the page ever asks a jack to say. Drawn as
// SVG so the bay and the sheet share the same shape, and as a block of its
// own where it sits in running text.

export function Jack({
  cx,
  cy,
  r,
  lit,
  className,
}: {
  cx: number;
  cy: number;
  r: number;
  lit: boolean;
  className?: string;
}) {
  return (
    <g className={className} data-lit={lit || undefined}>
      <circle cx={cx} cy={cy} r={r} className="pb-ring" />
      {lit ? <circle cx={cx} cy={cy} r={r * 0.4} className="pb-pin" /> : null}
    </g>
  );
}

export function JackInline({ lit, className }: { lit: boolean; className?: string }) {
  return (
    <svg viewBox="0 0 16 16" className={className} aria-hidden>
      <Jack cx={8} cy={8} r={6} lit={lit} />
    </svg>
  );
}
