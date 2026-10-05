// The Console exploration's palette, set on the same tokens the site reads so
// the fumadocs banner and nav follow it on this page alone. Grounds, lines and
// text are neutral white and black: the blue-violet is spent only where it
// means something (the prompt, the marks, the chapter playing, the primary
// action) and never tints a surface. To be refined; every value lives here
// and nowhere else.
export const light = {
  paper: '#ffffff',
  surface: '#f4f4f5',
  line: '#e4e4e7',
  ink: '#0a0a0a',
  muted: '#6b6b70',
  accent: '#5b4cf0',
  warn: '#b4640c',
  danger: '#c4403a',
};

export const dark = {
  paper: '#000000',
  surface: '#121213',
  line: '#26262a',
  ink: '#f5f5f5',
  muted: '#a1a1a8',
  accent: '#9f95ff',
  warn: '#e3a94a',
  danger: '#ec7a72',
};

const tokens = (palette: typeof light) =>
  Object.entries(palette)
    .map(([name, value]) => `--color-${name}:${value};`)
    .join('');

export const consoleTokens = `
:root{${tokens(light)}--cs-chip-ink:${light.paper};--cs-chip-paper:${light.ink};color-scheme:light}
html.dark{${tokens(dark)}--cs-chip-ink:${dark.paper};--cs-chip-paper:${dark.ink};color-scheme:dark}
body{background:var(--color-paper);color:var(--color-ink)}

.cs-caret{display:inline-block;width:.6em;height:1.15em;vertical-align:-.2em;background:var(--color-ink);animation:cs-blink 1.1s steps(1,end) infinite}
@keyframes cs-blink{50%{opacity:0}}
@media (prefers-reduced-motion:reduce){.cs-caret{animation:none}}

/* Scrollback: what has scrolled off the top goes quietly, never cut through a line. */
.cs-scrollback{mask-image:linear-gradient(to bottom,transparent,#000 2.5rem)}
`;
