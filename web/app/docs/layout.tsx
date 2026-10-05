import { source } from '@/lib/source';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { baseOptions } from '@/lib/layout.shared';
import { SidebarSection } from '@/components/sidebar-section';

export default function Layout({ children }: LayoutProps<'/docs'>) {
  return <DocsLayout tree={source.getPageTree()} sidebar={{ components: { Folder: SidebarSection } }} {...baseOptions()}>{children}</DocsLayout>;
}
