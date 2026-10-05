'use client';

import type { ReactNode } from 'react';
import type * as PageTree from 'fumadocs-core/page-tree';

// Every folder in the docs tree is a section of the sidebar, always open: a
// heading drawn like the `---Label---` separators (global.css styles both as
// `#nd-sidebar p`), then its pages beside the hairline fumadocs draws inside a
// folder. Collapsing hid the pages a reader came for, and fumadocs' own
// non-collapsible folder renders its title unstyled in the Base UI build.
// A fragment, so the heading is a sibling of the separators and takes the same
// rule above it.
export function SidebarSection({ item, children }: { item: PageTree.Folder; children: ReactNode }) {
  return (
    <>
      <p className="mt-6 mb-1 inline-flex items-center gap-2 px-2 [&_svg]:size-4 [&_svg]:shrink-0">
        {item.icon}
        {item.name}
      </p>
      <div className="relative flex flex-col gap-0.5 ps-4 before:absolute before:inset-y-1 before:start-2.5 before:w-px before:bg-fd-border before:content-['']">
        {children}
      </div>
    </>
  );
}
