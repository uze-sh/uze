import Link from 'next/link';

const variants = [
  { href: '/', name: 'Current', note: 'The home page as it is today.' },
  { href: '/lab/patchbay', name: 'Patch bay', note: 'One plugin routed to every agent, drawn as signal routing. Cobalt.' },
  { href: '/lab/manifest', name: 'Manifest', note: 'The project file as the hero: agents.yaml, set as a spec sheet. Violet.' },
  { href: '/lab/console', name: 'Console', note: 'The chosen direction: an interactive session as the hero, agents.yaml and the real workspace. Blue-violet.' },
];

export default function LabIndex() {
  return (
    <main className="mx-auto w-full max-w-2xl flex-1 px-6 py-20 font-sans">
      <h1 className="font-mono text-2xl font-bold text-ink">Home page explorations</h1>
      <ul className="mt-10 divide-y divide-line border-y border-line">
        {variants.map((variant) => (
          <li key={variant.href}>
            <Link href={variant.href} className="block py-5 transition-colors hover:text-accent">
              <span className="font-mono font-semibold text-ink">{variant.name}</span>
              <span className="mt-1 block text-sm text-muted">{variant.note}</span>
            </Link>
          </li>
        ))}
      </ul>
    </main>
  );
}
