import { harnessMarks } from '@/components/harness-marks';
import { Jack } from '@/components/lab/patchbay/jack';

// The hero's routing, drawn as the thing it is: one plugin on the left of the
// rack, patched into four agents, and each agent running on a lane of its
// own that lands on `main`. Two layouts, because the same picture at 390px
// is a wall of ten-pixel text: wide runs left to right beside the install
// module, compact hangs under it and fans the cables downward.
//
// Motion is one sequence on load, in CSS: cables draw in, jacks light, lanes
// extend, branch names appear. Base styles are the end state and the
// keyframes only describe where things start, so with reduced motion the
// picture is simply already there.

const BRANCHES = ['agent/k2x9fq', 'agent/07lmdo', 'agent/x1m2p9', 'agent/q8r4ts'];
// Where each lane has a commit on it, as a fraction of the lane's length.
// Uneven on purpose: four lanes with the same ticks read as a pattern, not
// as four agents at different points of their work.
const COMMITS = [[0.35, 0.72], [0.5], [0.22, 0.58, 0.86], [0.44]];

type Mark = (typeof harnessMarks)[number]['icon'];

function MarkGlyph({ icon, x, y, size }: { icon: Mark; x: number; y: number; size: number }) {
  return (
    <svg x={x} y={y} width={size} height={size} viewBox="0 0 24 24" aria-hidden>
      {icon.type === 'path' ? (
        <path d={icon.d} fill={icon.fill ?? 'currentColor'} />
      ) : (
        <image href={icon.href} width="24" height="24" />
      )}
    </svg>
  );
}

function Lane({
  x1,
  x2,
  y,
  index,
  label,
  labelSize,
}: {
  x1: number;
  x2: number;
  y: number;
  index: number;
  label: string;
  labelSize: number;
}) {
  const style = { '--i': index } as React.CSSProperties;
  return (
    <g style={style}>
      <line x1={x1} x2={x2} y1={y} y2={y} className="pb-lane-bed" />
      <line x1={x1} x2={x2} y1={y} y2={y} className="pb-lane" style={{ transformOrigin: `${x1}px ${y}px` }} />
      {COMMITS[index].map((at) => (
        <circle key={at} cx={x1 + (x2 - x1) * at} cy={y} r={3} className="pb-commit" />
      ))}
      <text x={x1} y={y - 9} className="pb-branch" fontSize={labelSize}>
        {label}
      </text>
    </g>
  );
}

function Agents({
  x,
  ys,
  cable,
  nameSize,
  iconSize,
}: {
  x: number;
  ys: number[];
  cable: (y: number) => string;
  nameSize: number;
  iconSize: number;
}) {
  return (
    <>
      {harnessMarks.map((harness, index) => {
        const y = ys[index];
        const style = { '--i': index } as React.CSSProperties;
        return (
          <g key={harness.name} className="pb-agent" data-i={index} style={style}>
            <path d={cable(y)} className="pb-cable-sleeve" />
            <path d={cable(y)} className="pb-cable" pathLength={1} />
            <Jack cx={x} cy={y} r={7} lit className="pb-jack" />
            <MarkGlyph icon={harness.icon} x={x + 16} y={y - iconSize / 2} size={iconSize} />
            <text x={x + 16 + iconSize + 7} y={y} dominantBaseline="central" className="pb-name" fontSize={nameSize}>
              {harness.name}
            </text>
          </g>
        );
      })}
    </>
  );
}

function Wide() {
  const ys = [56, 128, 200, 272];
  const agentX = 250;
  const laneX1 = 440;
  const laneX2 = 684;
  const railX = 694;
  // A patch cable leaves its jack straight before it bends: the first
  // control point holds the run flat past the module's edge, so four
  // cables share one trunk and fan out where the eye expects.
  const cable = (y: number) => `M7 164 C 150 164, 120 ${y}, ${agentX - 8} ${y}`;
  return (
    <svg viewBox="0 0 720 320" className="pb-bay pb-bay-wide" role="img" aria-labelledby="pb-bay-title">
      <title id="pb-bay-title">
        One plugin patched into Claude Code, Codex, OpenCode and Antigravity, each running on its own
        worktree branch that lands on main.
      </title>
      <text x={agentX - 8} y={22} className="pb-column">
        agents
      </text>
      <text x={laneX1} y={22} className="pb-column">
        worktrees
      </text>
      <text x={railX} y={22} textAnchor="middle" className="pb-column">
        main
      </text>
      <Agents x={agentX} ys={ys} cable={cable} nameSize={14} iconSize={16} />
      {ys.map((y, index) => (
        <Lane key={y} x1={laneX1} x2={laneX2} y={y} index={index} label={BRANCHES[index]} labelSize={11} />
      ))}
      <line x1={railX} x2={railX} y1={34} y2={296} className="pb-rail" />
    </svg>
  );
}

function Compact() {
  const ys = [112, 196, 280, 364];
  const agentX = 36;
  const laneX1 = 176;
  const laneX2 = 326;
  const railX = 336;
  // The cables start above the drawing, at the jack on the module's bottom
  // edge, sweep into one trunk down the left gutter and turn into each jack
  // from the side: a bundle, the way cables actually hang off a panel, and
  // nothing crosses a name.
  const cable = (y: number) =>
    `M180 -30 C 180 28, 10 16, 10 ${y - 30} Q 10 ${y}, ${agentX - 8} ${y}`;
  return (
    <svg viewBox="0 0 360 400" className="pb-bay pb-bay-compact" aria-hidden>
      <text x={agentX + 16} y={60} className="pb-column">
        agents
      </text>
      <text x={laneX1} y={60} className="pb-column">
        worktrees
      </text>
      <text x={railX} y={60} textAnchor="middle" className="pb-column">
        main
      </text>
      <Agents x={agentX} ys={ys} cable={cable} nameSize={13} iconSize={14} />
      {ys.map((y, index) => (
        <Lane key={y} x1={laneX1} x2={laneX2} y={y} index={index} label={BRANCHES[index]} labelSize={10} />
      ))}
      <line x1={railX} x2={railX} y1={72} y2={388} className="pb-rail" />
    </svg>
  );
}

export function Bay() {
  return (
    <div className="pb-bay-wrap">
      <Wide />
      <Compact />
    </div>
  );
}
