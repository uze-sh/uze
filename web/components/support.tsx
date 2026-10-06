// One mark per support level, read as "does it work" first: every level that
// works carries a check, and how it works is told by the fill (the harness's
// own mechanism is solid, UZE's adapter is an outline) and the hue (amber is a
// stated limit). Drawn as SVG in the page's own tokens, because emoji render
// differently on every platform and cannot follow the theme.

type Level = 'native' | 'uze' | 'partial' | 'none' | 'experimental' | 'planned';

const LABELS: Record<Level, string> = {
  native: 'Works natively',
  uze: 'Works through UZE',
  partial: 'Works, with a stated limit',
  none: 'Not available',
  experimental: 'Experimental',
  planned: 'Planned',
};

const CHECK = 'M7.5 12.5l3 3 6-6.5';

export function Support({ level, label }: { level: Level; label?: boolean }) {
  const title = LABELS[level];
  return (
    <span className="support" data-level={level} title={title} aria-label={title} role="img">
      <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
        {level === 'native' && (
          <>
            <circle cx="12" cy="12" r="10" className="support-fill" />
            <path d={CHECK} className="support-check-on-fill" />
          </>
        )}
        {(level === 'uze' || level === 'partial') && (
          <>
            <circle cx="12" cy="12" r="9.25" className="support-ring" />
            <path d={CHECK} className="support-check" />
          </>
        )}
        {level === 'none' && <path d="M8 12h8" className="support-dash" />}
        {level === 'experimental' && (
          <circle cx="12" cy="12" r="9.25" className="support-ring support-ring-dashed" />
        )}
        {level === 'planned' && <circle cx="12" cy="12" r="4" className="support-fill" />}
      </svg>
      {label && <span className="support-label">{title}</span>}
    </span>
  );
}

/** The legend: every level a table on the page uses, named once. */
export function SupportLegend({ levels }: { levels: Level[] }) {
  return (
    <div className="support-legend">
      {levels.map((level) => (
        <Support key={level} level={level} label />
      ))}
    </div>
  );
}
