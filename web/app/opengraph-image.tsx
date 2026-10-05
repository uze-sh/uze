import { ImageResponse } from 'next/og';
import { appDescription, appMotto, appName } from '@/lib/shared';

// The card a link to any page without one of its own unfurls into: the docs
// pages draw their own, so this is the landing page's, and every chat or
// feed that shows uze.sh shows this. Drawn from the site's own palette (the
// TUI's, see global.css) rather than a screenshot, so it stays legible at the
// thumbnail size most unfurls crop it to.
export const alt = `${appName}: ${appMotto}`;
export const size = { width: 1200, height: 630 };
export const contentType = 'image/png';

const ink = '#000000';
const paper = '#f5f5f5';
const muted = '#a1a1a8';
const line = '#26262a';

function Mark({ size: side }: { size: number }) {
  return (
    <svg width={side} height={side} viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg">
      <path
        d="M22.7 8.751 A2.6 2.6 0 0 1 25.3 8.751 L36.556 15.249 A2.6 2.6 0 0 1 37.856 17.501 L37.856 30.499 A2.6 2.6 0 0 1 36.556 32.751 L25.3 39.249 A2.6 2.6 0 0 1 22.7 39.249 L11.444 32.751 A2.6 2.6 0 0 1 10.144 30.499 L10.144 17.501 A2.6 2.6 0 0 1 11.444 15.249 Z"
        fill="none"
        stroke={paper}
        strokeWidth="3.2"
        strokeLinejoin="round"
      />
      <path
        d="M24.3 24.866 A1.2 1.2 0 0 1 24.9 23.827 L32.216 19.603 A1.2 1.2 0 0 1 34.016 20.642 L34.016 29.09 A1.2 1.2 0 0 1 33.416 30.129 L26.1 34.354 A1.2 1.2 0 0 1 24.3 33.314 Z"
        fill={paper}
      />
    </svg>
  );
}

function Module({ label, color }: { label: string; color: string }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
      <div style={{ width: 14, height: 14, borderRadius: 7, background: color }} />
      <div style={{ fontSize: 30, color: paper }}>{label}</div>
    </div>
  );
}

export default function Image() {
  return new ImageResponse(
    (
      <div
        style={{
          width: '100%',
          height: '100%',
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'space-between',
          background: ink,
          padding: '72px 80px',
          fontFamily: 'sans-serif',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 28 }}>
          <Mark size={120} />
          <div style={{ fontSize: 112, fontWeight: 700, color: paper, letterSpacing: -4 }}>{appName}</div>
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
          <div style={{ fontSize: 64, fontWeight: 700, color: paper, letterSpacing: -2, lineHeight: 1.05 }}>
            {appMotto}
          </div>
          <div style={{ fontSize: 30, color: muted, lineHeight: 1.35 }}>{appDescription}</div>
        </div>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            borderTop: `1px solid ${line}`,
            paddingTop: 32,
          }}
        >
          <div style={{ display: 'flex', gap: 48 }}>
            <Module label="Plugins" color={paper} />
            <Module label="Workspace" color={muted} />
          </div>
          <div style={{ fontSize: 30, color: muted }}>uze.sh</div>
        </div>
      </div>
    ),
    size,
  );
}
