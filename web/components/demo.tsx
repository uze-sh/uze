'use client';

import { type ReactNode, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { type CastHandle, CastPlayer } from '@/components/cast-player';

// A recording in the docs, framed the way the landing page frames its own:
// a bezel and a window's title bar, the app's ground inside. A cast is
// played as text (see CastPlayer), and a click opens it large in a dialog,
// from the second it was at; closing hands that second back. A GIF is what
// a page shows until its cast is recorded, and is fetched only by the media
// that matches it.
type Recording =
  | { cast: string; posterAt: number; src?: never; poster?: never }
  | { src: string; poster: string; cast?: never; posterAt?: never };

function Frame({ small, children }: { small?: boolean; children: ReactNode }) {
  const dot = small ? 'size-2.5' : 'size-3';
  return (
    <div className={`dark cast-frame bg-[#16181a] text-ink ${small ? 'rounded-[16px] p-1.5' : 'rounded-[22px] p-2'}`}>
      <div className={`overflow-hidden bg-[#0a0c0d] ring-1 ring-white/[0.06] ${small ? 'rounded-[11px]' : 'rounded-[14px]'}`}>
        <div
          className={`flex items-center border-b border-white/[0.06] ${small ? 'h-7 gap-1.5 px-3' : 'h-9 gap-2 px-4'}`}
          aria-hidden
        >
          <span className={`${dot} rounded-full bg-[#ff5f57]`} />
          <span className={`${dot} rounded-full bg-[#febc2e]`} />
          <span className={`${dot} rounded-full bg-[#28c840]`} />
        </div>
        {children}
      </div>
    </div>
  );
}

export function Demo({ alt, caption, ...recording }: Recording & { alt: string; caption?: string }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const inline = useRef<CastHandle | null>(null);
  const large = useRef<CastHandle | null>(null);
  const [from, setFrom] = useState<number | null>(null);

  const expand = () => {
    if (!recording.cast) return;
    inline.current?.pause();
    setFrom(inline.current?.getCurrentTime() ?? 0);
    dialog.current?.showModal();
  };
  const collapse = () => {
    const at = large.current?.getCurrentTime();
    setFrom(null);
    if (at !== undefined) void inline.current?.seek(at).then(() => inline.current?.play());
  };

  // The prose column's own width: bleeding past it ran under the sidebar,
  // whose edge the content padding does not clear.
  return (
    <figure className="not-prose my-8">
      {recording.cast ? (
        <>
          <div
            role="button"
            tabIndex={0}
            aria-label={`${alt}. Open larger`}
            onClick={expand}
            onKeyDown={(event) => (event.key === 'Enter' || event.key === ' ') && (event.preventDefault(), expand())}
            className="group cursor-zoom-in rounded-[16px] focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-accent"
          >
            <Frame small>
              <CastPlayer src={recording.cast} posterAt={recording.posterAt} label={alt} handle={inline} />
            </Frame>
          </div>
          {/* A native dialog: focus stays inside it while it is open. Sized by the screen's height as well as its
              width, at the terminal's shape, so the whole frame is in view. */}
          <dialog
            ref={dialog}
            onClose={collapse}
            // The player inside keeps the keyboard for its own shortcuts and
            // swallows Escape, so the dialog takes it first.
            onKeyDownCapture={(event) => event.key === 'Escape' && (event.preventDefault(), dialog.current?.close())}
            onClick={(event) => event.target === dialog.current && dialog.current.close()}
            className="m-auto max-h-none max-w-none bg-transparent p-0 backdrop:bg-black/70 backdrop:backdrop-blur-sm"
            style={{ width: 'min(92vw, calc((92svh - 3rem) * 1.95), 1600px)' }}
          >
            <button
              type="button"
              onClick={() => dialog.current?.close()}
              aria-label="Close"
              className="absolute -top-10 right-0 inline-flex size-8 items-center justify-center rounded-full text-white/70 transition-colors hover:text-white focus-visible:outline-2 focus-visible:outline-white"
            >
              <X className="size-5" strokeWidth={1.75} />
            </button>
            {from !== null ? (
              <Frame>
                <CastPlayer
                  src={recording.cast}
                  posterAt={recording.posterAt}
                  label={alt}
                  handle={large}
                  startAt={from}
                />
              </Frame>
            ) : null}
          </dialog>
        </>
      ) : (
        <Frame small>
          <picture className="block">
            <source srcSet={recording.poster} media="(prefers-reduced-motion: reduce)" />
            <img src={recording.src} alt={alt} className="block h-auto w-full" />
          </picture>
        </Frame>
      )}
      {caption ? (
        <figcaption className="mt-2 text-center text-xs text-fd-muted-foreground">{caption}</figcaption>
      ) : null}
    </figure>
  );
}
