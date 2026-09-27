'use client';
import { useTOCItems, TOCScrollArea } from 'fumadocs-ui/components/toc';
import { TOCItem, TOCItems } from 'fumadocs-ui/components/toc/default';
import { History } from 'lucide-react';

// Fumadocs' own TOC column with one word changed: on the changelog the
// right-hand list is the release history, and "On this page" would call a
// version a section of the page.
export function VersionsTOC() {
  const items = useTOCItems();

  return (
    <div
      id="nd-toc"
      className="sticky top-(--fd-docs-row-1) h-[calc(var(--fd-docs-height)-var(--fd-docs-row-1))] flex flex-col [grid-area:toc] w-(--fd-toc-width) pt-12 pe-4 pb-2 xl:layout:[--fd-toc-width:268px] max-xl:hidden"
    >
      <h3 id="toc-title" className="inline-flex items-center gap-1.5 text-sm text-fd-muted-foreground">
        <History className="size-4" />
        Versions
      </h3>
      <TOCScrollArea className="ms-px">
        <TOCItems>
          {items.map((item) => (
            <TOCItem key={item.url} item={item} />
          ))}
        </TOCItems>
      </TOCScrollArea>
    </div>
  );
}
