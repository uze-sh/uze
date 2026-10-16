import { HarnessMark, harnessMarks, homepageOf } from '@/components/harness-marks';

// The agents uze serves, set around the claim as tiles of glass: the claim is
// what stays, and they are what comes and goes around it. Placed off a grid
// on purpose, each at its own tilt and depth, so they read as objects in a
// space rather than as a row of logos. Only beside the full claim: on a
// phone they would sit on the words.
// Offsets from the claim's centre, in pixels: the claim is centred and sized
// by its words, so a share of its box lands on the words themselves.
const PLACEMENT: Record<string, { x: number; y: number; size: number; tilt: number; delay: string }> = {
  'Claude Code': { x: -540, y: -120, size: 76, tilt: -9, delay: '0s' },
  OpenCode: { x: -470, y: 130, size: 60, tilt: 7, delay: '-2.4s' },
  Codex: { x: 545, y: -95, size: 72, tilt: 8, delay: '-1.2s' },
  Antigravity: { x: 470, y: 150, size: 62, tilt: -6, delay: '-3.6s' },
};

export function HarnessOrbit() {
  return (
    <ul className="pointer-events-none absolute inset-0 hidden xl:block" aria-label="Works with">
      {harnessMarks.map((harness) => {
        const place = PLACEMENT[harness.name];
        return (
          <li
            key={harness.name}
            className="harness-float absolute"
            style={{
              left: `calc(50% + ${place.x}px)`,
              top: `calc(50% + ${place.y}px)`,
              animationDelay: place.delay,
              ['--tilt' as string]: `${place.tilt}deg`,
            }}
          >
            <a
              href={homepageOf(harness.name)}
              target="_blank"
              rel="noreferrer noopener"
              title={harness.name}
              className="glass-tile pointer-events-auto flex -translate-x-1/2 -translate-y-1/2 items-center justify-center text-ink focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-accent"
              style={{ width: place.size, height: place.size }}
            >
              <HarnessMark icon={harness.icon} className="size-[42%]" />
              <span className="sr-only">{harness.name}</span>
            </a>
          </li>
        );
      })}
    </ul>
  );
}
