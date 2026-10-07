import type { ReactNode } from 'react';

// The three extensions as the workspace draws them over the main area, after
// their recorded frames (web/public/uze-{spec,architect,code}-poster.png),
// each about the selected agent's checkout: the change it is working on, the
// diagram the project keeps, and the diff it left. `step` is the one gesture
// each takes on screen: a document picked, a box selected, a file picked.
// What a surface navigates by is drawn in the tab strip, by the workspace
// (`SurfaceNavigation`); inside it only its modes remain.

export type Surface = 'spec' | 'arch' | 'code';

function Hints({ items, trailing }: { items: [string, string][]; trailing?: string }) {
  return (
    <div className="flex shrink-0 items-center gap-x-2 overflow-hidden pt-2 text-[11px] whitespace-nowrap">
      {items.map(([key, label], i) => (
        <span key={key} className="text-muted">
          {i > 0 ? <span className="mr-2 text-line">·</span> : null}
          <span className="font-semibold text-ink">{key}</span> {label}
        </span>
      ))}
      {trailing ? <span className="ml-auto truncate pl-4 text-muted">{trailing}</span> : null}
    </div>
  );
}

function Modes({ modes, active }: { modes: string[]; active: string }) {
  return (
    <span className="ml-auto flex shrink-0 items-center gap-1 pl-3 max-md:hidden">
      {modes.map((mode) => (
        <span key={mode} className={`px-2 py-0.5 ${mode === active ? 'bg-surface font-semibold text-ink' : 'text-muted'}`}>
          {mode}
        </span>
      ))}
    </span>
  );
}

function Subjects({ subjects, active }: { subjects: string[]; active: string }) {
  return (
    <>
      {subjects.map((subject) => (
        <span
          key={subject}
          className={`px-2 py-0.5 ${subject === active ? 'bg-surface font-semibold text-ink' : 'text-muted'}`}
        >
          {subject}
        </span>
      ))}
    </>
  );
}

/**
 * What an open surface puts in the tab strip's leading slot, where the
 * agent's tabs stand otherwise: its subjects, or on a board the selector
 * and the level trail.
 */
export function SurfaceNavigation({ surface }: { surface: Surface }) {
  if (surface === 'arch') {
    return (
      <span className="flex min-w-0 items-center gap-2 whitespace-nowrap">
        <span className="bg-surface px-2 py-0.5 font-semibold text-ink">C4 ▾</span>
        <span className="text-muted">/</span>
        <span className="bg-surface px-2 py-0.5 font-semibold text-ink">System context</span>
        <span className="truncate text-muted max-md:hidden">› Containers › Router components</span>
      </span>
    );
  }
  return surface === 'spec' ? (
    <Subjects subjects={['changes', 'specs']} active="changes" />
  ) : (
    <Subjects subjects={['files', 'map', 'changes']} active="changes" />
  );
}

function Inline({ children }: { children: ReactNode }) {
  return <span className="text-info">{children}</span>;
}

function Group({ title, count, children }: { title: string; count: number; children?: ReactNode }) {
  return (
    <div className="mb-3">
      <div className="truncate font-semibold text-muted">
        <span className="mr-1.5">{children ? '▾' : '▸'}</span>
        {title} <span className="font-normal">{count}</span>
      </div>
      {children}
    </div>
  );
}

function Entry({
  children,
  trailing,
  selected,
  depth = 1,
}: {
  children: ReactNode;
  trailing?: ReactNode;
  selected?: boolean;
  depth?: 1 | 2;
}) {
  return (
    <div
      className={`flex items-center gap-2 py-px pr-1 ${depth === 2 ? 'pl-7' : 'pl-3'} ${selected ? 'bg-surface font-semibold text-ink' : 'text-ink'}`}
      style={selected ? { boxShadow: 'inset 2px 0 0 var(--color-accent)' } : undefined}
    >
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {trailing ? <span className="shrink-0">{trailing}</span> : null}
    </div>
  );
}

function Spec({ step }: { step: boolean }) {
  return (
    <>
      <div className="flex min-h-0 flex-1">
        <div className="w-[42%] max-w-52 shrink-0 overflow-hidden border-r border-line pr-2">
          <Group title="this checkout" count={1}>
            <Entry trailing={<span className="text-muted">2/4</span>}>add-not-found</Entry>
            <Entry depth={2} selected={!step}>
              proposal
            </Entry>
            <Entry depth={2} selected={step}>
              tasks
            </Entry>
          </Group>
          <Group title="in progress" count={1}>
            <Entry trailing={<span className="text-muted">3/7</span>}>add-rate-limiting</Entry>
          </Group>
          <Group title="ready to archive" count={1}>
            <Entry trailing={<span className="text-success">3/3</span>}>add-health-route</Entry>
          </Group>
          {/* The archive is the last band, folded until asked for. */}
          <Group title="archived" count={1} />
        </div>
        <div className="min-w-0 flex-1 overflow-hidden pl-3 text-ink">
          <div className="flex items-center">
            <span className="truncate text-muted">add-not-found/{step ? 'tasks.md' : 'proposal.md'}</span>
            <Modes modes={['Preview', 'Source']} active="Preview" />
          </div>
          {step ? (
            <div className="mt-2 space-y-1">
              <div className="font-semibold">1. Router</div>
              {(
                [
                  [
                    true,
                    <>
                      Add <Inline>not_found</Inline> to <Inline>src/router.rs</Inline>
                    </>,
                  ],
                  [
                    true,
                    <>
                      Answer <Inline>404</Inline> with the path asked for
                    </>,
                  ],
                  [
                    false,
                    <>
                      Cover unknown paths in <Inline>tests/router.rs</Inline>
                    </>,
                  ],
                  [
                    false,
                    <>
                      List the routes in <Inline>README.md</Inline>
                    </>,
                  ],
                ] as [boolean, ReactNode][]
              ).map(([done, text], i) => (
                <div key={i} className="flex gap-2">
                  <span className={`shrink-0 ${done ? 'text-success' : 'text-muted'}`}>{done ? '[x]' : '[ ]'}</span>
                  <span className={done ? 'text-muted' : ''}>{text}</span>
                </div>
              ))}
            </div>
          ) : (
            <div className="mt-2 space-y-2.5">
              <div className="font-semibold">Why</div>
              <p>A path nobody declared answers 200 with an empty body, so a typo reads as a success.</p>
              <div className="font-semibold">What Changes</div>
              <ul className="space-y-0.5">
                <li>
                  <span className="text-muted">• </span>Unknown paths answer <Inline>404 Not Found</Inline>.
                </li>
                <li>
                  <span className="text-muted">• </span>
                  <Inline>src/router.rs</Inline> ends its match with <Inline>not_found(path)</Inline>.
                </li>
              </ul>
            </div>
          )}
        </div>
      </div>
      <Hints
        items={[
          ['↓', 'next'],
          ['→', 'expand'],
          ['enter', 'open'],
          ['p', 'preview'],
          ['tab', 'next pane'],
          ['esc', 'close'],
        ]}
      />
    </>
  );
}

function Box({
  title,
  kind,
  text,
  lit,
  selected,
  className = '',
}: {
  title: string;
  kind: string;
  text: string;
  lit: boolean;
  selected?: boolean;
  className?: string;
}) {
  return (
    <div
      className={`rounded-md bg-paper px-3 py-2 text-center ${className}`}
      style={{
        boxShadow: `inset 0 0 0 ${selected ? 2 : 1}px var(--color-${lit ? 'ink' : 'line'})`,
      }}
    >
      <div className={`truncate font-semibold ${lit ? 'text-ink' : 'text-muted'}`}>{title}</div>
      <div className="truncate text-muted">[{kind}]</div>
      <div className={`mt-1.5 ${lit ? 'text-ink' : 'text-muted'}`}>{text}</div>
    </div>
  );
}

function Edge({ label, lit, at }: { label: string; lit: boolean; at: string }) {
  return (
    <div className="absolute inset-y-0 flex flex-col items-center" style={{ left: at }}>
      <div className={`w-px flex-1 ${lit ? 'bg-ink' : 'bg-muted'}`} />
      <span className={`leading-none ${lit ? 'text-ink' : 'text-muted'}`}>▼</span>
      <span className={`absolute top-1/3 left-2 whitespace-nowrap bg-paper ${lit ? 'text-ink' : 'text-muted'}`}>
        {label}
      </span>
    </div>
  );
}

function Arch({ step }: { step: boolean }) {
  return (
    <>
      <div className="flex shrink-0 pb-2">
        <Modes modes={['Unicode', 'ASCII', 'Source']} active="Unicode" />
      </div>
      {/* The canvas's dot grid, the same the terminal draws under a diagram. */}
      <div
        className="flex min-h-0 flex-1 flex-col justify-center px-[14%]"
        style={{
          backgroundImage: 'radial-gradient(var(--color-line) 1px, transparent 1px)',
          backgroundSize: '22px 22px',
        }}
      >
        <div className="grid grid-cols-2 gap-6">
          <Box title="Developer" kind="Person" text="Runs the service and reads its logs" lit />
          <Box title="web" kind="Software System" text="The site, calling the API" lit={step} />
        </div>
        <div className="relative h-16">
          <Edge label="Runs, reviews" lit={step} at="25%" />
          <Edge label="Calls /health" lit={step} at="75%" />
        </div>
        <Box
          title="api"
          kind="Software System"
          text="The HTTP service"
          lit
          selected={step}
          className="mx-auto w-[62%]"
        />
      </div>
      <Hints
        items={[
          ['esc', 'close'],
          ['o', 'artifacts'],
          ['tab', 'next artifact'],
          ['g', 'rendering'],
        ]}
        trailing="3 boxes · 2 edges · system-context.mmd"
      />
    </>
  );
}

type DiffLine = [number | null, '+' | '-' | ' ', ReactNode];

// Indentation is spelled as strings so a formatter cannot collapse it.
const NOT_FOUND: DiffLine[] = [
  [1, '+', 'use crate::response::Response;'],
  [2, '+', ''],
  [3, '+', 'pub fn not_found(path: &str) -> Response {'],
  [4, '+', <>{'    Response::text(404, format!('}<Inline>{'"no route for {path}"'}</Inline>{'))'}</>],
  [5, '+', '}'],
];

const ROUTER: DiffLine[] = [
  [4, ' ', 'pub fn route(path: &str) -> Response {'],
  [5, ' ', '    match path {'],
  [6, ' ', <>{'        '}<Inline>{'"/health"'}</Inline>{' => health::health(),'}</>],
  [null, '-', '        _ => Response::empty(200),'],
  [7, '+', '        _ => not_found(path),'],
  [8, ' ', '    }'],
  [9, ' ', '}'],
];

function Changes({ step }: { step: boolean }) {
  const file = step ? 'src/router.rs' : 'src/not_found.rs';
  const lines = step ? ROUTER : NOT_FOUND;
  return (
    <>
      <div className="flex min-h-0 flex-1">
        <div className="w-[38%] max-w-48 shrink-0 overflow-hidden border-r border-line pr-2">
          <Entry trailing={<span className="text-warn">M</span>}>README.md</Entry>
          <Entry selected={!step} trailing={<span className="text-success">U</span>}>
            not_found.rs <span className="font-normal text-muted">src</span>
          </Entry>
          <Entry selected={step} trailing={<span className="text-warn">M</span>}>
            router.rs <span className="font-normal text-muted">src</span>
          </Entry>
        </div>
        <div className="min-w-0 flex-1 overflow-hidden pl-3">
          <div className="truncate text-muted">DIFF · {file}</div>
          <div className="mt-2">
            {lines.map(([number, mark, text], i) => (
              <div
                key={i}
                className={`flex whitespace-pre ${mark === '+' ? 'bg-success/10' : mark === '-' ? 'bg-danger/10' : ''}`}
              >
                <span
                  className={`w-4 shrink-0 text-center ${mark === '+' ? 'text-success' : mark === '-' ? 'text-danger' : ''}`}
                >
                  {mark === ' ' ? '' : mark}
                </span>
                <span className="w-6 shrink-0 pr-2 text-right text-muted">{number ?? ''}</span>
                <span className={`overflow-hidden text-ellipsis whitespace-pre ${mark === '-' ? 'text-muted' : 'text-ink'}`}>
                  {text}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
      <Hints
        items={[
          ['↓', 'next'],
          ['.', 'actions'],
          ['m', 'map'],
          ['tab', 'next pane'],
          ['esc', 'close'],
        ]}
      />
    </>
  );
}

export function ExtensionSurface({ surface, step }: { surface: Surface; step: boolean }) {
  return (
    <section className="flex min-h-0 min-w-0 flex-1 flex-col p-3">
      {surface === 'spec' ? <Spec step={step} /> : surface === 'arch' ? <Arch step={step} /> : <Changes step={step} />}
    </section>
  );
}
