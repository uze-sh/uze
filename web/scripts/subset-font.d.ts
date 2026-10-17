// subset-font ships no types; this is the one call scripts/terminal-font.ts makes.
declare module 'subset-font' {
  export default function subsetFont(
    font: Buffer,
    text: string,
    options?: { targetFormat?: 'sfnt' | 'woff' | 'woff2' },
  ): Promise<Buffer>;
}
