import { CastPlayer } from '@/components/cast-player';

// A recording in the docs, framed the way the landing page frames its own:
// a bezel and a window's title bar, the app's ground inside. A cast is
// played as text (see CastPlayer); a GIF is what a page shows until its cast
// is recorded, and is fetched only by the media that matches it.
type Recording =
  | { cast: string; posterAt: number; src?: never; poster?: never }
  | { src: string; poster: string; cast?: never; posterAt?: never };

export function Demo({ alt, caption, ...recording }: Recording & { alt: string; caption?: string }) {
  // The prose column's own width: bleeding past it ran under the sidebar,
  // whose edge the content padding does not clear.
  return (
    <figure className="not-prose my-8">
      <div className="dark cast-frame rounded-[16px] bg-[#16181a] p-1.5 text-ink">
        <div className="overflow-hidden rounded-[11px] bg-[#0a0c0d] ring-1 ring-white/[0.06]">
          <div className="flex h-7 items-center gap-1.5 border-b border-white/[0.06] px-3" aria-hidden>
            <span className="size-2.5 rounded-full bg-[#ff5f57]" />
            <span className="size-2.5 rounded-full bg-[#febc2e]" />
            <span className="size-2.5 rounded-full bg-[#28c840]" />
          </div>
          {recording.cast ? (
            <CastPlayer src={recording.cast} posterAt={recording.posterAt} label={alt} />
          ) : (
            <picture className="block">
              <source srcSet={recording.poster} media="(prefers-reduced-motion: reduce)" />
              <img src={recording.src} alt={alt} className="block h-auto w-full" />
            </picture>
          )}
        </div>
      </div>
      {caption ? (
        <figcaption className="mt-2 text-center text-xs text-fd-muted-foreground">{caption}</figcaption>
      ) : null}
    </figure>
  );
}
