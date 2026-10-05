// The Manifest exploration's palette, scoped to its own route: a crisp white
// page with one violet, read in reverse on a deep ink-violet ground. Written
// as a plain stylesheet rather than Tailwind tokens so the fumadocs nav and
// banner, which read the same variables, follow it without a config change.

export const light = {
  paper: '#ffffff',
  ink: '#16133a',
  muted: '#5f5c7e',
  line: '#e4e2f0',
  surface: '#f6f5fc',
  accent: '#5b4cf0',
  warn: '#b4640c',
  danger: '#c4403a',
};

export const dark = {
  paper: '#0e0c1e',
  ink: '#f3f2fa',
  muted: '#a6a3c4',
  line: '#26233f',
  surface: '#171430',
  accent: '#9f95ff',
  warn: '#e3a94a',
  danger: '#ec7a72',
};

const tokens = (palette: typeof light) =>
  Object.entries(palette)
    .map(([name, value]) => `--color-${name}:${value};`)
    .join('');

const css = `
:root{${tokens(light)}color-scheme:light}
.dark{${tokens(dark)}color-scheme:dark}
body{background:var(--color-paper);color:var(--color-ink)}

.mf{font-family:var(--font-display),ui-sans-serif,system-ui,sans-serif;font-feature-settings:"ss01","cv11"}
.mf-display{font-weight:700;letter-spacing:-0.035em;line-height:0.98}
.mf-title{font-weight:600;letter-spacing:-0.022em;line-height:1.12}

.mf-cell{opacity:1}
.mf-plugin{background:color-mix(in srgb,var(--color-accent) 11%,transparent)}
.mf-rule{transform-origin:left}

@media (prefers-reduced-motion:no-preference){
  .mf-cell{opacity:0;animation:mf-in .4s cubic-bezier(.2,.7,.2,1) both;animation-delay:var(--d,0s)}
  .mf-plugin{background:transparent;animation:mf-lit .5s ease-out both;animation-delay:var(--d,0s)}
  .mf-rule{transform:scaleX(0);animation:mf-draw .7s cubic-bezier(.4,0,.2,1) both;animation-delay:var(--d,0s)}
}
@keyframes mf-in{from{opacity:0;transform:translateY(3px)}to{opacity:1;transform:none}}
@keyframes mf-lit{from{background:transparent}to{background:color-mix(in srgb,var(--color-accent) 11%,transparent)}}
@keyframes mf-draw{from{transform:scaleX(0)}to{transform:scaleX(1)}}

.mf details summary::-webkit-details-marker{display:none}
.mf a:focus-visible,.mf button:focus-visible,.mf summary:focus-visible{outline:2px solid var(--color-accent);outline-offset:3px;border-radius:2px}
`;

export function ManifestTheme() {
  return <style dangerouslySetInnerHTML={{ __html: css }} />;
}
