import { RootProvider } from 'fumadocs-ui/provider/next';
import { Banner } from 'fumadocs-ui/components/banner';
import './global.css';
import localFont from 'next/font/local';
import type { Metadata } from 'next';
import { appDescription, appTagline, appTitle, siteUrl } from '@/lib/shared';

// Inter from its own release (rsms/inter v4.1), self-hosted so the build
// fetches nothing. One variable file carries every weight the site uses;
// italics are rare enough here that the slant is synthesized rather than
// paying for a second file. OFL-1.1, beside it.
const inter = localFont({
  src: [{ path: './fonts/InterVariable.woff2', weight: '100 900', style: 'normal' }],
  variable: '--font-body',
});

// IBM's own release of Plex Mono, for the same hinting reason. OFL-1.1, beside it.
const plexMono = localFont({
  src: [
    { path: './fonts/IBMPlexMono-Regular.woff2', weight: '400' },
    { path: './fonts/IBMPlexMono-Medium.woff2', weight: '500' },
    { path: './fonts/IBMPlexMono-SemiBold.woff2', weight: '600' },
    { path: './fonts/IBMPlexMono-Bold.woff2', weight: '700' },
  ],
  variable: '--font-ui-mono',
});

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title: {
    default: `${appTitle} · ${appTagline}`,
    template: `%s · ${appTitle}`,
  },
  description: appDescription,
  openGraph: {
    type: 'website',
    siteName: appTitle,
    title: `${appTitle} · ${appTagline}`,
    description: appDescription,
  },
  twitter: {
    card: 'summary_large_image',
    title: `${appTitle} · ${appTagline}`,
    description: appDescription,
  },
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html
      lang="en"
      className={`${inter.variable} ${plexMono.variable} scrollbar-thin scrollbar-thumb-scrollbar scrollbar-track-transparent scrollbar-thumb-rounded-full`}
      suppressHydrationWarning
    >
      <body className="flex flex-col min-h-screen font-sans">
        <Banner
          id="beta-2026-09"
          height="var(--uze-banner-height)"
          className="gap-x-2 px-12 text-center font-mono text-xs tracking-tight"
        >
          <span className="text-accent">Beta</span>
          <span className="text-fd-muted-foreground">·</span>
          {/* One line at every width: the banner's height feeds the docs
              grid's sticky offsets, so text that wraps is text that gets
              clipped. */}
          <span className="sm:hidden">every release is a pre-release</span>
          <span className="max-sm:hidden">APIs and harness behavior are still changing</span>
          <span className="text-fd-muted-foreground max-sm:hidden">·</span>
          <span className="max-sm:hidden">every release until v1 is a pre-release</span>
        </Banner>
        <RootProvider>{children}</RootProvider>
      </body>
    </html>
  );
}
