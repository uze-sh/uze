export const appName = 'uze';
// What uze is, in the words a person arriving needs first: the category, the
// way Bun or Next.js open. It is the site's title too: a browser tab reading
// just "uze" says nothing to someone with twenty tabs open.
export const appTagline = 'the package manager and workspace for coding agents';
export const appDescription =
  'Install plugins once for Claude Code, Codex, OpenCode and Antigravity, and run them side by side, each in a checkout of its own.';
// Why uze exists, said after what it is: a principle, not a definition.
export const appMotto = 'Agents come and go. Your work stays.';
// The production deployment sets NEXT_PUBLIC_SITE_URL; the fallback is the
// same canonical domain, so a build without it never points unfurls and the
// sitemap at an address that no longer answers.
export const siteUrl = process.env.NEXT_PUBLIC_SITE_URL ?? 'https://uze.sh';
export const docsRoute = '/docs';
export const docsImageRoute = '/og/docs';
export const docsContentRoute = '/llms.mdx/docs';

export const gitConfig = {
  user: 'uze-sh',
  repo: 'uze',
  branch: 'main',
};