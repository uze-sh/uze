import Link from 'next/link';
import matrix from '@/lib/harness-matrix.json';
import { JackInline } from '@/components/lab/patchbay/jack';

// The delivery matrix, read as a patch sheet: a row per agent, a jack per
// capability, lit where the capability is patched through. The word under
// a lit jack is the route it took, because the question after "does it work
// here" is "how", and dimming a bridge against a native would answer a
// question nobody asked.

type Capability = 'context' | 'skills' | 'mcp' | 'agents' | 'hooks' | 'session' | 'package';

const columns: [Capability, string][] = [
  ['context', 'AGENTS.md'],
  ['skills', 'Skills'],
  ['mcp', 'MCP'],
  ['agents', 'Subagents'],
  ['hooks', 'Hooks'],
  ['session', 'Session'],
  ['package', 'Plugin'],
];

export function Sheet() {
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[40rem] border-separate border-spacing-0 text-left">
        <thead>
          <tr>
            <th scope="col" className="pb-4 pe-6 text-sm font-medium text-muted">
              Agent
            </th>
            {columns.map(([key, label]) => (
              <th key={key} scope="col" className="pb-4 pe-2 text-sm font-medium text-muted">
                {label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {matrix.harnesses.map((harness) => (
            <tr key={harness.name}>
              <th scope="row" className="border-t border-line py-4 pe-6 font-normal">
                <a
                  href={harness.url}
                  target="_blank"
                  rel="noreferrer noopener"
                  className="pb-link inline-flex items-center gap-2.5 text-ink"
                >
                  {harness.icon ? <img src={harness.icon} alt="" className="size-4 shrink-0" /> : null}
                  <span className="pb-display text-[15px] font-semibold">{harness.name}</span>
                </a>
              </th>
              {columns.map(([key]) => {
                const route = harness[key];
                const lit = route !== 'none';
                return (
                  <td key={key} className="border-t border-line py-4 pe-2 align-middle">
                    <span className="inline-flex items-center gap-2">
                      <JackInline lit={lit} className="size-4 shrink-0" />
                      <span className={`text-xs ${lit ? 'text-ink' : 'text-muted/70'}`}>
                        {lit ? route : 'other route'}
                      </span>
                    </span>
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
      <p className="mt-6 max-w-[70ch] text-sm leading-relaxed text-muted">
        An unlit jack means the capability does not travel this route for that agent and arrives
        another way. {matrix.planned.join(', ')} are on the roadmap.{' '}
        <Link href="/docs/reference/harnesses" className="pb-link text-ink underline underline-offset-4">
          The full matrix, per capability
        </Link>
        .
      </p>
    </div>
  );
}
