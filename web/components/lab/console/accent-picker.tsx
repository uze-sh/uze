'use client';

import { useEffect, useState } from 'react';

// A lab-only control for trying the primary colour on the real page: it
// overrides `--color-accent` and nothing else, so the neutral grounds stay
// what they are. Each pair is the light value and its lifted dark value, both
// at 4.5:1 or better on #fff and #000. Remembered per browser and readable
// from `?accent=`, so a link can point at one.
const accents = [
  // No brand hue: the primary is the ink itself, and colour is left to what
  // means something (a check, a diff, a warning), the way a terminal uses it.
  { id: 'mono', light: '#0a0a0a', dark: '#f5f5f5' },
  // The product's own: the default theme of uze's terminal UI
  // (crates/uze-theme/themes/default.json), deepened for a white page.
  { id: 'uze', light: '#2f7a4a', dark: '#8fd19e' },
  { id: 'indigo', light: '#5b4cf0', dark: '#9f95ff' },
  { id: 'cobalt', light: '#1f4fe0', dark: '#6f8fff' },
  { id: 'electric', light: '#0060f0', dark: '#4d9dff' },
  { id: 'teal', light: '#0a7d74', dark: '#2fd4c0' },
  { id: 'magenta', light: '#c8146c', dark: '#ff6aa8' },
  { id: 'orange', light: '#d4410f', dark: '#ff7a3d' },
  { id: 'amber', light: '#a35f00', dark: '#ffb224' },
  { id: 'lime', light: '#3f7d0a', dark: '#a3e635' },
];

const STORAGE_KEY = 'uze-lab-accent';

export function AccentPicker() {
  const [active, setActive] = useState('indigo');

  useEffect(() => {
    const fromUrl = new URLSearchParams(window.location.search).get('accent');
    let saved: string | null = null;
    try {
      saved = localStorage.getItem(STORAGE_KEY);
    } catch {}
    const initial = [fromUrl, saved].find((id) => accents.some((accent) => accent.id === id));
    if (initial) setActive(initial);
  }, []);

  const choose = (id: string) => {
    setActive(id);
    try {
      localStorage.setItem(STORAGE_KEY, id);
    } catch {}
  };

  const current = accents.find((accent) => accent.id === active) ?? accents[0];

  return (
    <>
      <style>{`:root{--color-accent:${current.light}}html.dark{--color-accent:${current.dark}}`}</style>
      <div
        role="radiogroup"
        aria-label="Primary colour"
        className="fixed right-4 bottom-4 z-50 flex items-center gap-1.5 border border-line bg-paper px-2.5 py-2 font-mono text-[11px] text-muted"
      >
        <span className="mr-1">accent</span>
        {accents.map((accent) => {
          const selected = accent.id === active;
          return (
            <button
              key={accent.id}
              type="button"
              role="radio"
              aria-checked={selected}
              aria-label={accent.id}
              title={`${accent.id}  ${accent.light} / ${accent.dark}`}
              onClick={() => choose(accent.id)}
              className="size-5 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
              style={{
                background: `linear-gradient(135deg, ${accent.light} 50%, ${accent.dark} 50%)`,
                boxShadow: selected ? '0 0 0 2px var(--color-paper), 0 0 0 3px var(--color-ink)' : 'none',
              }}
            />
          );
        })}
        <span className="ml-1 min-w-[4.5rem] text-ink">{current.id}</span>
      </div>
    </>
  );
}
