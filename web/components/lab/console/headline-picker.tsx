'use client';

import { useEffect, useState } from 'react';

// A lab-only control for judging headlines on the real hero, beside the
// session, instead of in the abstract. Remembered per browser and readable
// from `?h=`, so a link can point at one.
const headlines = [
  { id: 'motto', text: 'Agents come and go. Your work stays.' },
  { id: 'package', text: 'The package manager for coding agents.' },
];

const STORAGE_KEY = 'uze-lab-headline';

export function Headline({ className }: { className?: string }) {
  const [active, setActive] = useState(headlines[0].id);

  useEffect(() => {
    const fromUrl = new URLSearchParams(window.location.search).get('h');
    let saved: string | null = null;
    try {
      saved = localStorage.getItem(STORAGE_KEY);
    } catch {}
    const initial = [fromUrl, saved].find((id) => headlines.some((headline) => headline.id === id));
    if (initial) setActive(initial);
    const onPick = (event: Event) => setActive((event as CustomEvent<string>).detail);
    window.addEventListener('uze-lab-headline', onPick);
    return () => window.removeEventListener('uze-lab-headline', onPick);
  }, []);

  const current = headlines.find((headline) => headline.id === active) ?? headlines[0];
  // Each sentence starts its own line, so a break never lands mid-thought.
  const sentences = current.text.split(/(?<=\.)\s+/);
  return (
    <h1 className={className}>
      {sentences.map((sentence) => (
        <span key={sentence} className="block text-balance">
          {sentence}
        </span>
      ))}
    </h1>
  );
}

export function HeadlinePicker() {
  const [active, setActive] = useState(headlines[0].id);

  useEffect(() => {
    const fromUrl = new URLSearchParams(window.location.search).get('h');
    let saved: string | null = null;
    try {
      saved = localStorage.getItem(STORAGE_KEY);
    } catch {}
    const initial = [fromUrl, saved].find((id) => headlines.some((headline) => headline.id === id));
    if (initial) setActive(initial);
  }, []);

  const choose = (id: string) => {
    setActive(id);
    try {
      localStorage.setItem(STORAGE_KEY, id);
    } catch {}
    window.dispatchEvent(new CustomEvent('uze-lab-headline', { detail: id }));
  };

  return (
    <div
      role="radiogroup"
      aria-label="Headline"
      className="fixed right-4 bottom-16 z-50 flex items-center gap-1 border border-line bg-paper px-2.5 py-2 font-mono text-[11px] text-muted"
    >
      <span className="mr-1">headline</span>
      {headlines.map((headline) => {
        const selected = headline.id === active;
        return (
          <button
            key={headline.id}
            type="button"
            role="radio"
            aria-checked={selected}
            title={headline.text}
            onClick={() => choose(headline.id)}
            className={`px-1.5 py-0.5 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink ${
              selected ? 'bg-ink text-paper' : 'hover:text-ink'
            }`}
          >
            {headline.id}
          </button>
        );
      })}
    </div>
  );
}
