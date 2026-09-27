import { DocsBody, DocsDescription, DocsPage, DocsTitle } from 'fumadocs-ui/layouts/docs/page';
import { TOCPopover, TOCProvider } from 'fumadocs-ui/layouts/docs/page/slots/toc';
import { Markdown } from 'fumadocs-core/content/md';
import { remarkHeading } from 'fumadocs-core/mdx-plugins';
import type { Metadata } from 'next';
import { getMDXComponents } from '@/components/mdx';
import { VersionsTOC } from '@/components/versions-toc';
import { loadChangelog } from '@/lib/changelog';

const title = 'Changelog';
const description = 'Every uze release, newest first';

export const metadata: Metadata = { title, description };

export default function Page() {
  const { body, versions } = loadChangelog();

  return (
    <DocsPage
      toc={versions}
      slots={{ toc: { provider: TOCProvider, main: VersionsTOC, popover: TOCPopover } }}
    >
      <DocsTitle>{title}</DocsTitle>
      <DocsDescription>{description}</DocsDescription>
      <DocsBody>
        <Markdown remarkPlugins={[remarkHeading]} components={getMDXComponents()}>
          {body}
        </Markdown>
      </DocsBody>
    </DocsPage>
  );
}
