import { ArrowUpRight } from 'lucide-react';
import { Frame } from '@/components/demo';

// A catalog of extensions a marketplace carries: each one shown at work, where
// it stands in the workspace and what it may reach, with its own README for
// the rest. Rows are parted by hairlines, as the site's structure is, rather
// than boxed.
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
    <ul className="not-prose my-8 divide-y divide-fd-border border-y border-fd-border">
      {items.map((extension) => (
        <li key={extension.id} className="grid gap-5 py-6 sm:grid-cols-[minmax(0,1.15fr)_minmax(0,1fr)] sm:items-center">
          <a href={extension.href} className="block rounded-[16px] focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-accent">
            <Frame small>
              <img src={extension.gif} alt="" loading="lazy" width={1649} height={980} className="block h-auto w-full" />
            </Frame>
          </a>
          <div className="flex min-w-0 flex-col gap-3">
            <h3 className="font-mono text-base font-semibold text-fd-foreground">{extension.id}</h3>
            <p className="text-sm text-fd-muted-foreground">{extension.summary}</p>
            <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-xs">
              <dt className="text-fd-muted-foreground">Shows in</dt>
              <dd className="flex flex-wrap gap-1.5">
                {extension.slots.map((slot) => (
                  <span key={slot} className="rounded-md border border-fd-border px-1.5 font-mono text-fd-foreground">
                    {slot}
                  </span>
                ))}
              </dd>
              <dt className="text-fd-muted-foreground">May</dt>
              <dd className="text-fd-foreground">{extension.may}</dd>
            </dl>
            <a
              href={extension.href}
              className="inline-flex w-fit items-center gap-1 text-sm font-medium text-fd-foreground underline-offset-4 hover:underline"
            >
              README
              <ArrowUpRight className="size-3.5" strokeWidth={1.75} />
            </a>
          </div>
        </li>
      ))}
    </ul>
  );
}
