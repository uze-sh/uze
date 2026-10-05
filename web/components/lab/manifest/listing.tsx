import Link from 'next/link';
import { HarnessMark, harnessMarks, homepageOf } from '@/components/harness-marks';
import matrix from '@/lib/harness-matrix.json';

// The hero's one object: a project's agents.yaml, what `uze install` resolved
// it into for each agent, and the agents.lock that pins it, set as one
// continuous listing with one line-number gutter. A reader who knows a
// package.json and a lockfile recognises the shape before reading a word.
// The file is this repository's own, trimmed to the keys the page talks
// about; the routes come from the matrix the integrations generate, so the
// picture cannot claim a route the code stopped taking.

type Capability = 'context' | 'skills' | 'mcp' | 'agents' | 'hooks' | 'session' | 'package';

const COLUMNS: [Capability, string][] = [
  ['context', 'AGENTS.md'],
  ['skills', 'Skills'],
  ['mcp', 'MCP'],
  ['agents', 'Subagents'],
  ['hooks', 'Hooks'],
  ['session', 'Session'],
  ['package', 'Plugin'],
];

type Tone = 'key' | 'value' | 'punct' | 'plain';
type Token = [Tone, string];
type Line = { tokens: Token[]; lit?: boolean };

const yaml = (key: string, value?: string, indent = 0): Line => ({
  tokens: value === undefined
    ? [['plain', ' '.repeat(indent)], ['key', key], ['punct', ':']]
    : [['plain', ' '.repeat(indent)], ['key', key], ['punct', ': '], ['value', value]],
});
const item = (value: string, indent: number, lit = false): Line => ({
  tokens: [['plain', ' '.repeat(indent)], ['punct', '- '], ['value', value]],
  lit,
});
const blank = (): Line => ({ tokens: [] });

const MANIFEST: Line[] = [
  yaml('worktrees'),
  yaml('default', 'isolated', 2),
  yaml('completion', 'pr', 2),
  blank(),
  yaml('marketplaces'),
  yaml('ai', undefined, 2),
  yaml('git', 'https://github.com/hiukky/ai', 4),
  yaml('plugins', undefined, 4),
  item('git', 6, true),
  item('tui', 6, true),
];

const REVISION = '66db2e8d1f3a23e7b6c676cec32688a8157b88f4';
const LOCK: Line[] = [
  yaml('marketplaces'),
  yaml('ai', undefined, 2),
  yaml('git', 'https://github.com/hiukky/ai', 4),
  yaml('revision', REVISION, 4),
  yaml('plugins'),
  yaml('git', undefined, 2),
  yaml('integrity', 'sha256:09654d7ed6e64c83c4b832254c0207a8be81d13a8c8ab5e81a636b9a56d168f9', 4),
  yaml('tui', undefined, 2),
  yaml('integrity', 'sha256:aab7de3c377ebd36aea0dbec2562ec1577e18da05f60b44895db01e9d7907d57', 4),
];

// The page-load sequence, in seconds: the plugin lines light, the install
// rule draws, the ledger fills in row by row, the lock follows. One moment,
// and nothing else on the page moves on its own.
const LIT_AT = 0.5;
const INSTALL_AT = 0.9;
const LEDGER_AT = 1.25;
const ROW_STEP = 0.16;
const CELL_STEP = 0.04;
const LOCK_AT = LEDGER_AT + 4 * ROW_STEP + 0.3;

const toneClass: Record<Tone, string> = {
  key: 'text-ink',
  value: 'text-accent',
  punct: 'text-muted',
  plain: '',
};

const routeOf = (harness: string, capability: Capability) =>
  (matrix.harnesses.find((entry) => entry.name === harness)?.[capability] ?? 'none') as string;

// A 40-character hash is the one thing in these files wider than a phone or a tablet,
// so it is shortened there and nowhere else.
function Hash({ value }: { value: string }) {
  const [prefix, digest] = value.includes(':') ? value.split(':') : ['', value];
  return (
    <>
      {prefix ? `${prefix}:` : ''}
      <span className="lg:hidden">{digest.slice(0, 14)}…</span>
      <span className="max-lg:hidden">{digest}</span>
    </>
  );
}

function Gutter({ n }: { n?: number }) {
  return (
    <span aria-hidden className="w-9 shrink-0 select-none pe-3 text-right text-[0.78em] tabular-nums text-muted/55 sm:w-11">
      {n ?? '\u00a0'}
    </span>
  );
}

function CodeLine({ line, n, delay }: { line: Line; n: number; delay?: number }) {
  return (
    <div className={`flex items-baseline ${line.lit ? 'mf-plugin' : ''}`} style={line.lit ? ({ '--d': `${LIT_AT}s` } as React.CSSProperties) : undefined}>
      <Gutter n={n} />
      <code className={`whitespace-pre ${delay !== undefined ? 'mf-cell' : ''}`} style={delay !== undefined ? ({ '--d': `${delay}s` } as React.CSSProperties) : undefined}>
        {line.tokens.length === 0 ? ' ' : null}
        {line.tokens.map(([tone, text], i) => (
          <span key={i} className={toneClass[tone]}>
            {text.length >= 40 ? <Hash value={text} /> : text}
          </span>
        ))}
      </code>
    </div>
  );
}

// The three parts are named where a reader's eye lands, on the line itself,
// in the page's sans rather than the file's mono: a file name and a note
// about who writes it are the page talking, not the file.
function Caption({ n, name, note, delay }: { n: number; name: string; note: string; delay?: number }) {
  return (
    <div className="mt-7 flex items-baseline first:mt-0">
      <Gutter n={n} />
      <div className={`mf flex flex-wrap items-baseline gap-x-3 gap-y-0.5 ${delay !== undefined ? 'mf-cell' : ''}`} style={delay !== undefined ? ({ '--d': `${delay}s` } as React.CSSProperties) : undefined}>
        <span className="text-[0.95em] font-semibold text-ink">{name}</span>
        <span className="text-[0.82em] text-muted">{note}</span>
      </div>
    </div>
  );
}

function Route({ value, delay }: { value: string; delay: number }) {
  const translated = value === 'bridge' || value === 'adapted';
  const none = value === 'none';
  return (
    <span
      className={`mf-cell inline-flex items-center gap-1.5 whitespace-nowrap text-[0.82em] ${none ? 'text-muted/60' : 'text-ink'}`}
      style={{ '--d': `${delay}s` } as React.CSSProperties}
      title={none ? 'not delivered through this route' : translated ? 'delivered by another mechanism, and uze reports it' : 'delivered through the agent’s own mechanism'}
    >
      <span
        aria-hidden
        className={`size-[0.55em] shrink-0 ${none ? 'rounded-full border border-current' : translated ? 'rotate-45 bg-warn' : 'bg-accent'}`}
      />
      {value}
    </span>
  );
}

export function Listing() {
  // The gutter counts straight through the three parts. The ledger is drawn
  // twice, once per breakpoint, and both are the same lines, so the numbers
  // are laid out once here rather than counted while rendering.
  const manifestAt = 2;
  const installAt = manifestAt + MANIFEST.length;
  const ledgerCaptionAt = installAt + 1;
  const ledgerAt = ledgerCaptionAt + 1;
  const legendAt = ledgerAt + matrix.harnesses.length;
  const lockCaptionAt = legendAt + 1;
  const lockAt = lockCaptionAt + 1;

  return (
    <figure
      aria-label="A project's agents.yaml declaring two plugins from one marketplace; uze install resolving it into Claude Code, Codex, OpenCode and Antigravity, with what each received; and the agents.lock pinning the marketplace to a commit and each plugin to a digest."
      className="w-full font-mono text-[13px] leading-[1.75] sm:text-[15px] lg:text-[17px]"
    >
      <Caption n={1} name="agents.yaml" note="committed with the project, written by you" />
      <div className="mt-2">
        {MANIFEST.map((line, i) => (
          <CodeLine key={i} line={line} n={manifestAt + i} />
        ))}
      </div>

      <div className="mt-6 flex items-baseline">
        <Gutter n={installAt} />
        <div className="flex min-w-0 flex-1 items-baseline gap-4">
          <code className="mf-cell whitespace-nowrap text-ink" style={{ '--d': `${INSTALL_AT}s` } as React.CSSProperties}>
            <span className="text-accent">$ </span>uze install
          </code>
          <span aria-hidden className="mf-rule h-px flex-1 translate-y-[-0.35em] bg-accent" style={{ '--d': `${INSTALL_AT + 0.1}s` } as React.CSSProperties} />
        </div>
      </div>

      {/* The ledger: one row per agent, one column per capability, the route
          word in each cell. On a phone each agent is one line of the listing
          holding its own list, so the gutter still counts straight through. */}
      <Caption n={ledgerCaptionAt} name="What each agent received" note="the route in each cell is read from the integration that implements it" delay={LEDGER_AT - 0.2} />
      <div className="mt-2 max-sm:hidden">
        <div className="flex items-baseline">
          <Gutter />
          <div className="mf grid flex-1 grid-cols-[9.5rem_repeat(7,minmax(0,1fr))] gap-x-3 text-[0.78em] text-muted">
            <span />
            {COLUMNS.map(([key, label]) => (
              <span key={key} className="mf-cell" style={{ '--d': `${LEDGER_AT - 0.1}s` } as React.CSSProperties}>
                {label}
              </span>
            ))}
          </div>
        </div>
        {matrix.harnesses.map((harness, row) => (
          <div key={harness.name} className="flex items-baseline">
            <Gutter n={ledgerAt + row} />
            <div className="mf grid flex-1 grid-cols-[9.5rem_repeat(7,minmax(0,1fr))] items-baseline gap-x-3 border-t border-line py-1.5">
              <Agent name={harness.name} delay={LEDGER_AT + row * ROW_STEP} />
              {COLUMNS.map(([key], col) => (
                <Route key={key} value={routeOf(harness.name, key)} delay={LEDGER_AT + row * ROW_STEP + (col + 1) * CELL_STEP} />
              ))}
            </div>
          </div>
        ))}
      </div>
      <div className="mt-2 sm:hidden">
        {matrix.harnesses.map((harness, row) => (
          <div key={harness.name} className="flex items-baseline">
            <Gutter n={ledgerAt + row} />
            <div className="mf flex-1 border-t border-line py-2">
              <Agent name={harness.name} delay={LEDGER_AT + row * ROW_STEP} />
              <ul className="mt-1.5 grid grid-cols-2 gap-x-3 gap-y-1">
                {COLUMNS.map(([key, label], col) => (
                  <li key={key} className="flex items-baseline justify-between gap-2 text-[0.82em]">
                    <span className="text-muted">{label}</span>
                    <Route value={routeOf(harness.name, key)} delay={LEDGER_AT + row * ROW_STEP + (col + 1) * CELL_STEP} />
                  </li>
                ))}
              </ul>
            </div>
          </div>
        ))}
      </div>
      <div className="flex items-baseline">
        <Gutter n={legendAt} />
        <p className="mf mf-cell max-w-[62ch] text-[0.82em] leading-relaxed text-muted" style={{ '--d': `${LOCK_AT - 0.2}s` } as React.CSSProperties}>
          Violet arrived through the agent&apos;s own mechanism. Amber was delivered another way, and
          uze tells you which.{' '}
          <Link href="/docs/reference/harnesses" className="text-ink underline decoration-line underline-offset-4 hover:decoration-accent">
            The full matrix, per capability
          </Link>
          .
        </p>
      </div>

      <Caption n={lockCaptionAt} name="agents.lock" note="written by uze, never edited, regenerated when deleted" delay={LOCK_AT} />
      <div className="mt-2">
        {LOCK.map((line, i) => (
          <CodeLine key={i} line={line} n={lockAt + i} delay={LOCK_AT + 0.05 + i * 0.03} />
        ))}
      </div>
    </figure>
  );
}

function Agent({ name, delay }: { name: string; delay: number }) {
  const mark = harnessMarks.find((harness) => harness.name === name);
  return (
    <a
      href={homepageOf(name)}
      target="_blank"
      rel="noreferrer noopener"
      className="mf-cell inline-flex items-center gap-2 whitespace-nowrap text-[0.95em] font-semibold text-ink hover:text-accent"
      style={{ '--d': `${delay}s` } as React.CSSProperties}
    >
      {mark ? <HarnessMark icon={mark.icon} className="size-[1.05em] shrink-0" /> : null}
      {name}
    </a>
  );
}
