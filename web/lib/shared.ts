export const appName = 'uze';
// The site's title and its unfurl. The motto leads, the way the home page
// does: the pain uze answers is being tied to one agent, and a category noun
// ("package manager") put uze beside tools it only half resembles. The
// description says literally what it is. A browser tab reading just "uze"
// says nothing to someone with twenty tabs open.
export const appMotto = 'Agents come and go. Your work stays.';
export const appTagline = appMotto;
export const appDescription =
  'A layer between you and your coding agents: the same plugins and AGENTS.md in Claude Code, Codex, OpenCode and Antigravity, and a terminal to run them side by side.';
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