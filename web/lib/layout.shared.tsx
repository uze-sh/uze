import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { appName, gitConfig } from './shared';
import { UzeMark } from '@/components/uze-mark';
import { GitHubStars } from '@/components/github-stars';

// Where the release stands, from the workspace Cargo.toml's version (see
// next.config.mjs): the major and the pre-release's name, `1.0.0-beta.15`
// as "v1 beta", so the badge follows the release without being edited.
const release = (() => {
  const match = process.env.NEXT_PUBLIC_UZE_VERSION?.match(/^(\d+)\.\d+\.\d+(?:-([a-z]+))?/);
  return match ? [`v${match[1]}`, match[2]].filter(Boolean).join(' ') : null;
})();

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: (
        // The brand leads the header: a mark the height of the word, the
        // word a size above the links, and a hairline closing it off from
        // them, so it reads as the site's name rather than its first link.
        <span className="brand-title inline-flex items-center gap-2.5 border-fd-border pe-1 font-mono text-[17px] font-bold tracking-[-0.03em] text-fd-foreground md:me-1 md:border-e md:pe-5">
          <UzeMark className="size-[1.15em] text-accent" />
          {appName}
          {/* Where the release stands, not which build: the full version is
              the changelog's subject and sat beside the name as noise. */}
          {release ? (
            <span className="rounded-full px-2 py-px text-[10px] font-semibold tracking-normal text-fd-muted-foreground ring-1 ring-fd-border">
              {release}
            </span>
          ) : null}
        </span>
      ),
    },
    // The way into the docs, where the header has room for it: the only
    // other one was "Get started" in the hero, and readers did not find it.
    // Only in the home's header: the docs' own sidebar already lists the
    // changelog and is the docs.
    links: [
      { text: 'Docs', url: '/docs', active: 'nested-url', on: 'nav' },
      { text: 'Changelog', url: '/docs/changelog', on: 'nav' },
    ],
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  };
}

// The home's header, where the repository's link also carries its stars:
// the icon-only `githubUrl` button has no room for a count. The docs keep
// that button, in the sidebar's foot where readers already find it.
export function homeOptions(): BaseLayoutProps {
  const base = baseOptions();
  return {
    ...base,
    githubUrl: undefined,
    links: [...(base.links ?? []), { type: 'custom', secondary: true, children: <GitHubStars /> }],
  };
}
