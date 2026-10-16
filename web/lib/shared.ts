export const appName = 'uze';
// The name as text beside other words: a tab or an unfurl title. The
// lowercase form is the wordmark, drawn where it stands alone; set in a
// sentence-cased title it reads as a typo.
export const appTitle = 'UZE';
// The site's title and its unfurl. The motto leads, the way the home page
// does: the pain uze answers is being tied to one agent, and a category noun
// ("package manager") put uze beside tools it only half resembles. The
// description says literally what it is. A browser tab reading just "uze"
// says nothing to someone with twenty tabs open.
export const appMotto = 'Agents come and go. Your work stays.';
export const appTagline = appMotto;
export const appDescription =
  'The terminal workspace for coding agents: the same plugins and AGENTS.md in Claude Code, Codex, OpenCode and Antigravity, running side by side, each in its own worktree.';
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