import type { Metadata } from 'next';
import type { ReactNode } from 'react';

// Design explorations for the home page, kept out of search and the sitemap:
// they are compared here and either replace the home page or are deleted.
export const metadata: Metadata = {
  title: 'Lab',
  robots: { index: false, follow: false },
};

export default function LabLayout({ children }: { children: ReactNode }) {
  return children;
}
