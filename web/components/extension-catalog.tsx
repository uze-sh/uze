import { ArrowUpRight } from 'lucide-react';
import { Frame } from '@/components/demo';

// A catalog of extensions a marketplace carries: each one shown at work, where
// it stands in the workspace and what it may reach, with its own README for
// the rest. Each is a card, as the page's own Cards are: parted by hairlines
// alone, a row's facts read as the start of the next one.
type Extension = {
  id: string;
  summary: string;
  gif: string;
  slots: string[];
  may: string;
  href: string;
};

export function ExtensionCatalog({ items }: { items: Extension[] }) {
  return (
    <ul className="not-prose my-8 flex flex-col gap-4">
      {items.map((extension) => (
        <li key={extension.id} className="grid gap-5 rounded-xl border border-fd-border bg-fd-card p-3 sm:grid-cols-[minmax(0,1.15fr)_minmax(0,1fr)] sm:p-4">
          <a href={extension.href} className="block rounded-[16px] focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-accent">
            <Frame small>
              <img src={extension.gif} alt="" loading="lazy" width={1649} height={980} className="block h-auto w-full" />
            </Frame>
          </a>
          <div className="flex min-w-0 flex-col justify-between gap-5 px-1 pb-1 sm:py-1 sm:pe-2">
            <div className="flex flex-col gap-2">
              <div className="flex items-baseline justify-between gap-3">
                <h3 className="font-mono text-base font-semibold text-fd-foreground">{extension.id}</h3>
                <a
                  href={extension.href}
                  className="inline-flex shrink-0 items-center gap-1 text-xs text-fd-muted-foreground transition-colors hover:text-fd-foreground"
                >
                  README
                  <ArrowUpRight className="size-3" strokeWidth={1.75} />
                </a>
              </div>
              <p className="text-sm leading-relaxed text-fd-muted-foreground">{extension.summary}</p>
            </div>
            <dl className="flex flex-col gap-3 text-xs">
              <div className="flex flex-col gap-1.5">
                <dt className="text-[0.6875rem] uppercase tracking-wider text-fd-muted-foreground">Shows in</dt>
                <dd className="flex flex-wrap gap-1.5">
                  {extension.slots.map((slot) => (
                    <span key={slot} className="rounded-md border border-fd-border bg-fd-background px-1.5 py-px font-mono text-fd-foreground">
                      {slot}
                    </span>
                  ))}
                </dd>
              </div>
              <div className="flex flex-col gap-1.5">
                <dt className="text-[0.6875rem] uppercase tracking-wider text-fd-muted-foreground">May</dt>
                <dd className="leading-relaxed text-fd-foreground">{extension.may}</dd>
              </div>
            </dl>
          </div>
        </li>
      ))}
    </ul>
  );
}
