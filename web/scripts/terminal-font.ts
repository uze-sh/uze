// Cuts the font the site's recordings are drawn in down to the glyphs they
// print, and to those of the home page's takes. The workspace draws Nerd
// Font icons, so the face is FiraCode Nerd Font Mono, and the whole face is
// 2.7MB a weight; every cast together uses a few hundred glyphs. Run after
// publishing a cast or a take:
//
//   bun run terminal-font
//
// The release is pinned by version and digest, and kept in the system's
// temporary directory between runs: what is fetched is checked before a byte
// of it is used.
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { unzipSync } from 'fflate';
import subsetFont from 'subset-font';

const RELEASE = 'https://github.com/ryanoasis/nerd-fonts/releases/download/v3.5.1/FiraCode.zip';
const DIGEST = '239395baf60c89b2eaf4862b6b09db0ef95605cd3e8eef51c00345822a81a665';
const WEIGHTS = ['Regular', 'Bold'] as const;

const web = fileURLToPath(new URL('..', import.meta.url));
const fonts = join(web, 'public/fonts');
const stylesheet = join(web, 'app/global.css');
const casts = listCasts(join(web, 'public/casts'));
const takes = join(web, 'lib/home-takes.json');

function listCasts(dir: string): string[] {
  return existsSync(dir) ? readdirSync(dir).filter((f) => f.endsWith('.cast')).map((f) => join(dir, f)) : [];
}

// What a cast prints, without the escape sequences that carry no glyph:
// CSI (colours, cursor moves), OSC (titles) and the two-byte escapes.
function printed(cast: string): Set<string> {
  const glyphs = new Set<string>();
  for (const line of readFileSync(cast, 'utf8').split('\n').slice(1)) {
    if (!line.trim()) continue;
    const [, kind, data] = JSON.parse(line) as [number, string, string];
    if (kind !== 'o') continue;
    const text = data
      .replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, '')
      .replace(/\x1b\][^\x07\x1b]*(\x07|\x1b\\)?/g, '')
      .replace(/\x1b./g, '');
    for (const ch of text) if (ch >= ' ') glyphs.add(ch);
  }
  return glyphs;
}

async function release(): Promise<Uint8Array> {
  const cached = join(tmpdir(), `nerd-fonts-${DIGEST.slice(0, 12)}.zip`);
  const bytes = existsSync(cached)
    ? readFileSync(cached)
    : new Uint8Array(await (await fetch(RELEASE)).arrayBuffer());
  const digest = createHash('sha256').update(bytes).digest('hex');
  if (digest !== DIGEST) throw new Error(`${RELEASE}: digest ${digest}, expected ${DIGEST}`);
  if (!existsSync(cached)) writeFileSync(cached, bytes);
  return bytes;
}

const text = new Set<string>();
for (let c = 0x20; c < 0x7f; c++) text.add(String.fromCharCode(c));
for (const cast of casts) for (const ch of printed(cast)) text.add(ch);
for (const rows of Object.values(JSON.parse(readFileSync(takes, 'utf8')).takes) as [string, number][][][])
  for (const row of rows) for (const [run] of row) for (const ch of run) if (ch >= ' ') text.add(ch);

const archive = unzipSync(await release());
mkdirSync(fonts, { recursive: true });
// The stylesheet names each face by its content: a subset keeps its file
// name, and a browser that had the previous one cached kept drawing it, so
// a glyph a new take needed was blank until a hard reload.
let css = readFileSync(stylesheet, 'utf8');
for (const weight of WEIGHTS) {
  const face = archive[`FiraCodeNerdFontMono-${weight}.ttf`];
  const subset = await subsetFont(Buffer.from(face), [...text].join(''), { targetFormat: 'woff2' });
  const file = `FiraCodeNerdMono-${weight}.subset.woff2`;
  writeFileSync(join(fonts, file), subset);
  const version = createHash('sha256').update(subset).digest('hex').slice(0, 12);
  css = css.replace(new RegExp(`/fonts/${file}(\\?v=[0-9a-f]+)?`), `/fonts/${file}?v=${version}`);
  console.log(`${weight}: ${text.size} glyphs from ${casts.length} casts, ${Math.round(subset.length / 1024)}KB`);
}
writeFileSync(stylesheet, css);
writeFileSync(join(fonts, 'FiraCode-OFL.txt'), archive['LICENSE']);
