import type { MetadataRoute } from 'next';
import { source } from '@/lib/source';
import { siteUrl } from '@/lib/shared';

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { url: siteUrl },
    // The changelog is its own route, rendered from CHANGELOG.md, so `source` does not list it.
    { url: new URL('/docs/changelog', siteUrl).toString() },
    ...source.getPages().map((page) => ({ url: new URL(page.url, siteUrl).toString() })),
  ];
}
