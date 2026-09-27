import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { appName, gitConfig } from './shared';
import { UzeMark } from '@/components/uze-mark';

// Injected from the workspace Cargo.toml at build time (see next.config.mjs).
const version = process.env.NEXT_PUBLIC_UZE_VERSION;

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: (
        <span className="font-mono font-semibold tracking-tight text-fd-foreground">
          <UzeMark className="mr-2 inline-block size-[0.75em] align-middle text-accent" />
          {appName}
          {version ? (
            <span className="ml-2 text-[11px] font-normal text-fd-muted-foreground">
              v{version}
            </span>
          ) : null}
        </span>
      ),
    },
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  };
}
