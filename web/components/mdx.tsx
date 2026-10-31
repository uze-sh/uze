import defaultMdxComponents from 'fumadocs-ui/mdx';
import { Tab, Tabs, TabsList, TabsTrigger } from 'fumadocs-ui/components/tabs';
import { Accordion, Accordions } from 'fumadocs-ui/components/accordion';
import { File, Files, Folder } from 'fumadocs-ui/components/files';
import { Mermaid } from './mermaid';
import { Enforced, EnforcedLegend, Support, SupportLegend } from './support';
import { Demo } from './demo';
import { ExtensionCatalog } from './extension-catalog';
import type { MDXComponents } from 'mdx/types';

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    Tabs,
    Tab,
    TabsList,
    TabsTrigger,
    Accordions,
    Accordion,
    Mermaid,
    Support,
    SupportLegend,
    Enforced,
    EnforcedLegend,
    Demo,
    ExtensionCatalog,
    Files,
    File,
    Folder,
    ...components,
  } satisfies MDXComponents;
}

export const useMDXComponents = getMDXComponents;

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>;
}
