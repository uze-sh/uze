import { source } from '@/lib/source';
import { llms } from 'fumadocs-core/source';
import { appDescription } from '@/lib/shared';

export const revalidate = false;

// `uze --help` points agents here, so this is often the first thing an agent
// reads about uze: it says what uze is and how to read the rest as Markdown
// before it lists the pages.
const preamble = `# uze

> ${appDescription}

Every page below is also served as Markdown: add \`.md\` to its path, as in
\`/docs/quickstart.md\`. The whole documentation in one file is \`/llms-full.txt\`.
`;

export function GET() {
  const index = llms(source).index().replace(/^# Docs\n+/, '');
  return new Response(`${preamble}\n${index}`);
}
