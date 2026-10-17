import type { ReactNode } from 'react'

/**
 * The YouTube Music app's glyphs: WestPlay's own drawings, in the manner of YouTube Music's (filled shapes,
 * rounded ends), on a 24-unit grid, in the text's colour.
 */

function Glyph({ size, children, label }: { size: number; children: ReactNode; label?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden={label ? undefined : true}
      role={label ? 'img' : undefined}
      aria-label={label}
    >
      {children}
    </svg>
  )
}

type Size = { size?: number }

/** The app's mark: a red disc, a ring and a play sign (drawn here, not YouTube's file). */
export function MarkGlyph({ size = 30 }: Size) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden>
      <circle cx="12" cy="12" r="11" fill="#ff0033" />
      <circle cx="12" cy="12" r="6" fill="none" stroke="#fff" strokeWidth="1.2" />
      <path d="M10.4 9.3 15 12l-4.6 2.7z" fill="#fff" />
    </svg>
  )
}

export function HomeGlyph({ size = 26, filled = false }: Size & { filled?: boolean }) {
  return (
    <Glyph size={size}>
      {filled ? (
        <path d="M4 10.2 12 4l8 6.2V19a1 1 0 0 1-1 1h-4.5v-6h-5v6H5a1 1 0 0 1-1-1z" />
      ) : (
        <path d="M12 4 4 10.2V19a1 1 0 0 0 1 1h5.5v-6h3v6H19a1 1 0 0 0 1-1v-8.8zm0 2.5 6 4.7V18h-2.5v-6h-7v6H6v-6.8z" />
      )}
    </Glyph>
  )
}

export function ExploreGlyph({ size = 26, filled = false }: Size & { filled?: boolean }) {
  return (
    <Glyph size={size}>
      {filled ? (
        <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm3.9 6.1-2.2 5.6-5.6 2.2 2.2-5.6zM12 10.8a1.2 1.2 0 1 0 0 2.4 1.2 1.2 0 0 0 0-2.4z" />
      ) : (
        <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 2a8 8 0 1 1 0 16 8 8 0 0 1 0-16zm4 4-5.4 2.6L8 16l5.4-2.6zm-4 2.8a1.2 1.2 0 1 1 0 2.4 1.2 1.2 0 0 1 0-2.4z" />
      )}
    </Glyph>
  )
}

export function LibraryGlyph({ size = 26, filled = false }: Size & { filled?: boolean }) {
  return (
    <Glyph size={size}>
      {filled ? (
        <path d="M4 6h2v14h14v2H6a2 2 0 0 1-2-2zm4-4h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2zm3.5 4v7.3a2.2 2.2 0 1 0 1.5 2.1V8.5h3V6z" />
      ) : (
        <path d="M4 6h2v14h14v2H6a2 2 0 0 1-2-2zm4-4h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2zm0 2v12h12V4zm3.5 2h4.5v2.5h-3v6.9a2.2 2.2 0 1 1-1.5-2.1z" />
      )}
    </Glyph>
  )
}

export function SearchGlyph({ size = 26 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M10 3a7 7 0 0 1 5.6 11.2l5.1 5.1-1.4 1.4-5.1-5.1A7 7 0 1 1 10 3zm0 2a5 5 0 1 0 0 10 5 5 0 0 0 0-10z" />
    </Glyph>
  )
}

export function BackGlyph({ size = 26 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M20 11H7.8l5.6-5.6L12 4l-8 8 8 8 1.4-1.4L7.8 13H20z" />
    </Glyph>
  )
}

export function CloseGlyph({ size = 24 }: Size) {
  return (
    <Glyph size={size}>
      <path d="m6.4 5 5.6 5.6L17.6 5 19 6.4 13.4 12l5.6 5.6-1.4 1.4-5.6-5.6L6.4 19 5 17.6l5.6-5.6L5 6.4z" />
    </Glyph>
  )
}

export function ChevronDownGlyph({ size = 30 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M5.4 8.6 12 15.2l6.6-6.6L20 10l-8 8-8-8z" />
    </Glyph>
  )
}

/** A searched-for words' row: the clock of the past. */
export function HistoryGlyph({ size = 24 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M13 3a9 9 0 1 1-8.5 12h2.1A7 7 0 1 0 6 9.5l2.5 2.5H2V5.5l2.6 2.6A9 9 0 0 1 13 3zm-1 4h1.5v5.2l4 2.4-.8 1.3-4.7-2.9z" />
    </Glyph>
  )
}

/** A suggestion's arrow, up to the field: its words into it. */
export function FillGlyph({ size = 24 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M17.6 19 7 8.4V15H5V5h10v2H8.4L19 17.6z" />
    </Glyph>
  )
}

export function PlayGlyph({ size = 26 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M8 5.1v13.8a.8.8 0 0 0 1.2.7l10.6-6.9a.8.8 0 0 0 0-1.4L9.2 4.4A.8.8 0 0 0 8 5.1z" />
    </Glyph>
  )
}

export function PauseGlyph({ size = 26 }: Size) {
  return (
    <Glyph size={size}>
      <rect x="6" y="4.5" width="4" height="15" rx="1" />
      <rect x="14" y="4.5" width="4" height="15" rx="1" />
    </Glyph>
  )
}

export function NextGlyph({ size = 26 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M5 6.2v11.6a.8.8 0 0 0 1.2.7l8.5-5.8a.8.8 0 0 0 0-1.4L6.2 5.5A.8.8 0 0 0 5 6.2zM16.5 5.5h2a.5.5 0 0 1 .5.5v12a.5.5 0 0 1-.5.5h-2a.5.5 0 0 1-.5-.5V6a.5.5 0 0 1 .5-.5z" />
    </Glyph>
  )
}

export function PreviousGlyph({ size = 26 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M19 6.2v11.6a.8.8 0 0 1-1.2.7l-8.5-5.8a.8.8 0 0 1 0-1.4l8.5-5.8a.8.8 0 0 1 1.2.7zM7.5 5.5h-2a.5.5 0 0 0-.5.5v12a.5.5 0 0 0 .5.5h2a.5.5 0 0 0 .5-.5V6a.5.5 0 0 0-.5-.5z" />
    </Glyph>
  )
}

export function ShuffleGlyph({ size = 26 }: Size) {
  return (
    <Glyph size={size}>
      <path
        d="M3.5 7h3.2c2 0 3 1.2 4.3 3.5S13.8 17 16 17h2.5M3.5 17h3.2c2 0 3-1.2 4.3-3.5S13.8 7 16 7h2.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
      <path d="M17.5 4 21 7l-3.5 3zm0 10 3.5 3-3.5 3z" />
    </Glyph>
  )
}

export function RepeatGlyph({ size = 26, one = false }: Size & { one?: boolean }) {
  return (
    <Glyph size={size}>
      <path d="M7 7h10V4l4 4-4 4V9H9v4H7zm10 10H7v3l-4-4 4-4v3h8v-4h2z" />
      {one && <path d="M11.2 10.5h1.6v5h-1.4v-3.4l-.9.3v-1.1z" />}
    </Glyph>
  )
}

export function ThumbUpGlyph({ size = 24, filled = false }: Size & { filled?: boolean }) {
  return (
    <Glyph size={size}>
      {filled ? (
        <path d="M2 10h3.5v11H2zm5.5 11V10l5.2-7.6a1.6 1.6 0 0 1 2.9 1.2L14.4 9H20a2 2 0 0 1 2 2.3l-1.4 8A2 2 0 0 1 18.6 21z" />
      ) : (
        <path d="M2 10h3.5v11H2zm5.5 11V10l5.2-7.6a1.6 1.6 0 0 1 2.9 1.2L14.4 9H20a2 2 0 0 1 2 2.3l-1.4 8A2 2 0 0 1 18.6 21zm2-10.4V19h9.1l1.4-8h-8.2l1.6-6.1z" />
      )}
    </Glyph>
  )
}

export function ThumbDownGlyph({ size = 24, filled = false }: Size & { filled?: boolean }) {
  return (
    <Glyph size={size}>
      <g transform="rotate(180 12 12)">
        {filled ? (
          <path d="M2 10h3.5v11H2zm5.5 11V10l5.2-7.6a1.6 1.6 0 0 1 2.9 1.2L14.4 9H20a2 2 0 0 1 2 2.3l-1.4 8A2 2 0 0 1 18.6 21z" />
        ) : (
          <path d="M2 10h3.5v11H2zm5.5 11V10l5.2-7.6a1.6 1.6 0 0 1 2.9 1.2L14.4 9H20a2 2 0 0 1 2 2.3l-1.4 8A2 2 0 0 1 18.6 21zm2-10.4V19h9.1l1.4-8h-8.2l1.6-6.1z" />
        )}
      </g>
    </Glyph>
  )
}

/** Saving to the Library: a plus over lines; saved, a tick. */
export function SaveGlyph({ size = 24, saved = false }: Size & { saved?: boolean }) {
  return (
    <Glyph size={size}>
      <path d="M3 6h12v2H3zm0 4h12v2H3zm0 4h8v2H3z" />
      {saved ? (
        <path d="m14.5 16.8 2 2 4.6-4.6 1.4 1.4-6 6-3.4-3.4z" />
      ) : (
        <path d="M17 13h2v3h3v2h-3v3h-2v-3h-3v-2h3z" />
      )}
    </Glyph>
  )
}

/** A radio: waves from a point. */
export function RadioGlyph({ size = 24 }: Size) {
  return (
    <Glyph size={size}>
      <circle cx="12" cy="12" r="2.2" />
      <path d="M7.8 7.8l1.4 1.4a4 4 0 0 0 0 5.6l-1.4 1.4a6 6 0 0 1 0-8.4zm8.4 0a6 6 0 0 1 0 8.4l-1.4-1.4a4 4 0 0 0 0-5.6zM5 5l1.4 1.4a8 8 0 0 0 0 11.2L5 19a10 10 0 0 1 0-14zm14 0a10 10 0 0 1 0 14l-1.4-1.4a8 8 0 0 0 0-11.2z" />
    </Glyph>
  )
}

export function PersonGlyph({ size = 26 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 4a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7zm0 14a8 8 0 0 1-6-2.7c.8-1.9 3.4-3.1 6-3.1s5.2 1.2 6 3.1A8 8 0 0 1 12 20z" />
    </Glyph>
  )
}

/** The song playing, in Up next: three bars. */
export function EqualizerGlyph({ size = 20 }: Size) {
  return (
    <Glyph size={size}>
      <rect className="ytm-eq-bar" x="4" y="9" width="3.4" height="11" rx="1" />
      <rect className="ytm-eq-bar" x="10.3" y="4" width="3.4" height="16" rx="1" />
      <rect className="ytm-eq-bar" x="16.6" y="12" width="3.4" height="8" rx="1" />
    </Glyph>
  )
}

/** A song's ⋮: its menu. */
export function MoreGlyph({ size = 24 }: Size) {
  return (
    <Glyph size={size}>
      <circle cx="12" cy="5.5" r="1.9" />
      <circle cx="12" cy="12" r="1.9" />
      <circle cx="12" cy="18.5" r="1.9" />
    </Glyph>
  )
}

/** Play next: a song in front of the list. */
export function PlayNextGlyph({ size = 24 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M3 5h12v2H3zm0 4h12v2H3zm0 4h8v2H3zm11 0 7 4.5-7 4.5z" />
    </Glyph>
  )
}

/** Add to queue: a song at the list's end. */
export function QueueGlyph({ size = 24 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M3 5h14v2H3zm0 4h14v2H3zm0 4h9v2H3zm13 0h2v3h3v2h-3v3h-2v-3h-3v-2h3z" />
    </Glyph>
  )
}

/** An album: a record. */
export function AlbumGlyph({ size = 24 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M12 3a9 9 0 1 1 0 18 9 9 0 0 1 0-18zm0 2a7 7 0 1 0 0 14 7 7 0 0 0 0-14zm0 4.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5z" />
    </Glyph>
  )
}

/** An artist: a person. */
export function ArtistGlyph({ size = 24 }: Size) {
  return (
    <Glyph size={size}>
      <path d="M12 4a4 4 0 1 1 0 8 4 4 0 0 1 0-8zm0 2a2 2 0 1 0 0 4 2 2 0 0 0 0-4zm0 8c3.6 0 7 1.8 7 4.5V20H5v-1.5C5 15.8 8.4 14 12 14zm0 2c-2.6 0-4.6 1.1-4.9 2h9.8c-.3-.9-2.3-2-4.9-2z" />
    </Glyph>
  )
}

/** The volume: a speaker, quiet (no waves) or loud (two). */
export function VolumeGlyph({ size = 24, loud = false }: Size & { loud?: boolean }) {
  return (
    <Glyph size={size}>
      <path d="M4 9h4l5-4v14l-5-4H4z" />
      {loud && (
        <path d="M15.5 8.5a5 5 0 0 1 0 7l-1.4-1.4a3 3 0 0 0 0-4.2zm2.8-2.8a9 9 0 0 1 0 12.6l-1.4-1.4a7 7 0 0 0 0-9.8z" />
      )}
    </Glyph>
  )
}
