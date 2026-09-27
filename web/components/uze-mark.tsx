import type { ComponentProps } from 'react';

// uze's mark, drawn in `currentColor` so the caller's text colour decides its
// hue. The viewBox is cropped to the hexagon itself (the favicon's is the full
// 48-unit tile), so it sits on the text baseline without a margin of its own.
export function UzeMark(props: ComponentProps<'svg'>) {
  return (
    <svg viewBox="6.5 6.5 35 35" aria-hidden {...props}>
      <path
        d="M22.7 8.751 A2.6 2.6 0 0 1 25.3 8.751 L36.556 15.249 A2.6 2.6 0 0 1 37.856 17.501 L37.856 30.499 A2.6 2.6 0 0 1 36.556 32.751 L25.3 39.249 A2.6 2.6 0 0 1 22.7 39.249 L11.444 32.751 A2.6 2.6 0 0 1 10.144 30.499 L10.144 17.501 A2.6 2.6 0 0 1 11.444 15.249 Z"
        fill="none"
        stroke="currentColor"
        strokeWidth="3.2"
        strokeLinejoin="round"
      />
      <path
        d="M24.3 24.866 A1.2 1.2 0 0 1 24.9 23.827 L32.216 19.603 A1.2 1.2 0 0 1 34.016 20.642 L34.016 29.09 A1.2 1.2 0 0 1 33.416 30.129 L26.1 34.354 A1.2 1.2 0 0 1 24.3 33.314 Z"
        fill="currentColor"
      />
    </svg>
  );
}
