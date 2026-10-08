import { ImageResponse } from 'next/og';
import { appMotto, appName } from '@/lib/shared';

// An unfurl is glanced at before its title and description are read. This card
// therefore keeps only the identity, the promise, and the address; every
// element that does not help one of those three has been removed.
export const alt = `${appName}: ${appMotto}`;
export const size = { width: 1200, height: 630 };
export const contentType = 'image/png';

const ink = '#000000';
const paper = '#f5f5f5';
const muted = '#a1a1a8';

function Mark({ size: side, color }: { size: number; color: string }) {
  return (
    <svg width={side} height={side} viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg">
      <path
        d="M22.7 8.751 A2.6 2.6 0 0 1 25.3 8.751 L36.556 15.249 A2.6 2.6 0 0 1 37.856 17.501 L37.856 30.499 A2.6 2.6 0 0 1 36.556 32.751 L25.3 39.249 A2.6 2.6 0 0 1 22.7 39.249 L11.444 32.751 A2.6 2.6 0 0 1 10.144 30.499 L10.144 17.501 A2.6 2.6 0 0 1 11.444 15.249 Z"
        fill="none"
        stroke={color}
        strokeWidth="3.2"
        strokeLinejoin="round"
      />
      <path
        d="M24.3 24.866 A1.2 1.2 0 0 1 24.9 23.827 L32.216 19.603 A1.2 1.2 0 0 1 34.016 20.642 L34.016 29.09 A1.2 1.2 0 0 1 33.416 30.129 L26.1 34.354 A1.2 1.2 0 0 1 24.3 33.314 Z"
        fill={color}
      />
    </svg>
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
          padding: '58px 72px',
          background: ink,
          color: paper,
          fontFamily: 'sans-serif',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
          <div style={{ display: 'flex', marginTop: 4 }}>
            <Mark size={45} color={paper} />
          </div>
          <div style={{ marginBottom: 4, fontSize: 48, fontWeight: 700, letterSpacing: -2.5, lineHeight: 1 }}>{appName}</div>
        </div>

        <div style={{ display: 'flex', flex: 1, alignItems: 'center', justifyContent: 'center' }}>
          <div style={{ display: 'flex', flexDirection: 'column', width: 820, gap: 2 }}>
            <div style={{ fontSize: 76, fontWeight: 700, color: muted, letterSpacing: -4, lineHeight: 1.02 }}>Agents come and go.</div>
            <div style={{ fontSize: 76, fontWeight: 700, color: paper, letterSpacing: -4, lineHeight: 1.02 }}>Your work stays.</div>
          </div>
        </div>

        <div style={{ display: 'flex', fontFamily: 'monospace', fontSize: 19, color: muted }}>uze.sh</div>
      </div>
    ),
    size,
  );
}
