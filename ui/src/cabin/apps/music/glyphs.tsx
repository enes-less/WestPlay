import type { ReactNode } from 'react'

/**
 * Playback glyphs, shared by the Music app and the Dashboard's Now Playing widget. Filled shapes with softly
 * rounded corners (a stroke in the fill colour rounds them), heavy enough to read on the truck screen.
 */

interface GlyphProps {
  size: number
}

function Glyph({ size, children }: GlyphProps & { children: ReactNode }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="currentColor"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinejoin="round"
      aria-hidden
    >
      {children}
    </svg>
  )
}

export function PlayGlyph({ size }: GlyphProps) {
  return (
    <Glyph size={size}>
      <path d="M7 4.8v14.4a.9.9 0 0 0 1.37.77l11.3-7.2a.9.9 0 0 0 0-1.54l-11.3-7.2A.9.9 0 0 0 7 4.8Z" />
    </Glyph>
  )
}

export function PauseGlyph({ size }: GlyphProps) {
  return (
    <Glyph size={size}>
      <rect x="5.6" y="4.6" width="4.2" height="14.8" rx="1.2" />
      <rect x="14.2" y="4.6" width="4.2" height="14.8" rx="1.2" />
    </Glyph>
  )
}

export function NextGlyph({ size }: GlyphProps) {
  return (
    <Glyph size={size}>
      <path d="M2.6 6.8v10.4a.8.8 0 0 0 1.23.67L11.6 12.7a.8.8 0 0 0 0-1.4L3.83 6.13a.8.8 0 0 0-1.23.67Z" />
      <path d="M12.4 6.8v10.4a.8.8 0 0 0 1.23.67l7.77-5.17a.8.8 0 0 0 0-1.4l-7.77-5.17a.8.8 0 0 0-1.23.67Z" />
    </Glyph>
  )
}

export function PreviousGlyph({ size }: GlyphProps) {
  return (
    <span style={{ display: 'inline-flex', transform: 'scaleX(-1)' }}>
      <NextGlyph size={size} />
    </span>
  )
}

/** Line glyphs: the screen's secondary controls (back, queue, shuffle, repeat). */
function LineGlyph({ size, children }: GlyphProps & { children: ReactNode }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={2.2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {children}
    </svg>
  )
}

export function BackGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="M14.5 5 7.5 12l7 7" />
    </LineGlyph>
  )
}

export function QueueGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="M9 6.5h11M9 12h11M9 17.5h11" />
      <circle cx="4.5" cy="6.5" r="0.6" fill="currentColor" />
      <circle cx="4.5" cy="12" r="0.6" fill="currentColor" />
      <circle cx="4.5" cy="17.5" r="0.6" fill="currentColor" />
    </LineGlyph>
  )
}

export function ShuffleGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="M3 7h3.5c4 0 6 10 10 10H21M3 17h3.5c1.6 0 2.8-1.6 3.8-3.6M13.7 9.6c1-2 2.2-3.6 3.8-3.6H21" />
      <path d="m18.5 3.5 2.5 2.5-2.5 2.5M18.5 14.5l2.5 2.5-2.5 2.5" />
    </LineGlyph>
  )
}

export function RepeatGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="M4 11V9.5A3.5 3.5 0 0 1 7.5 6H20M20 13v1.5a3.5 3.5 0 0 1-3.5 3.5H4" />
      <path d="m17 3 3 3-3 3M7 15l-3 3 3 3" />
    </LineGlyph>
  )
}

export function RepeatOneGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="M4 11V9.5A3.5 3.5 0 0 1 7.5 6H20M20 13v1.5a3.5 3.5 0 0 1-3.5 3.5H4" />
      <path d="m17 3 3 3-3 3M7 15l-3 3 3 3" />
      <path d="M11 10.5 12.4 9.6v4.6" strokeWidth={1.8} />
    </LineGlyph>
  )
}

export function CheckGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="m5 12.5 4.5 4.5L19 7.5" />
    </LineGlyph>
  )
}

/** A music note: the artwork placeholder while a track has none. */
export function NoteGlyph({ size }: GlyphProps) {
  return (
    <Glyph size={size}>
      <path d="M9 17.5V6.2l10-2.2v11.3" fill="none" strokeWidth={2} />
      <ellipse cx="6.6" cy="17.6" rx="2.6" ry="2.1" />
      <ellipse cx="16.6" cy="15.4" rx="2.6" ry="2.1" />
    </Glyph>
  )
}

// --- Apple Music's pages (our own drawings, in Apple's manner) ---------------------------------------------

/** Home: a house, filled while chosen. */
export function HomeGlyph({ size, on = false }: GlyphProps & { on?: boolean }) {
  return on ? (
    <Glyph size={size}>
      <path d="M3.5 11.2 12 4l8.5 7.2V19a1.3 1.3 0 0 1-1.3 1.3H15v-5.6h-6v5.6H4.8A1.3 1.3 0 0 1 3.5 19Z" />
    </Glyph>
  ) : (
    <LineGlyph size={size}>
      <path d="M3.8 11.2 12 4.3l8.2 6.9V19a1.2 1.2 0 0 1-1.2 1.2H15v-5.5H9v5.5H5A1.2 1.2 0 0 1 3.8 19Z" />
    </LineGlyph>
  )
}

/** New: four tiles, filled while chosen. */
export function NewGlyph({ size, on = false }: GlyphProps & { on?: boolean }) {
  const tiles = [
    [3.5, 3.5],
    [13, 3.5],
    [3.5, 13],
    [13, 13],
  ]
  return on ? (
    <Glyph size={size}>
      {tiles.map(([x, y]) => (
        <rect key={`${x}${y}`} x={x} y={y} width="7.5" height="7.5" rx="2" />
      ))}
    </Glyph>
  ) : (
    <LineGlyph size={size}>
      {tiles.map(([x, y]) => (
        <rect key={`${x}${y}`} x={x + 0.4} y={y + 0.4} width="6.7" height="6.7" rx="1.8" />
      ))}
    </LineGlyph>
  )
}

/** Radio: a mast with waves either side. */
export function RadioGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <circle cx="12" cy="12" r="1.6" fill="currentColor" />
      <path d="M8.3 15.7a5.2 5.2 0 0 1 0-7.4M15.7 8.3a5.2 5.2 0 0 1 0 7.4M5.4 18.6a9.3 9.3 0 0 1 0-13.2M18.6 5.4a9.3 9.3 0 0 1 0 13.2" />
    </LineGlyph>
  )
}

/** Library: a note on a stack of squares. */
export function LibraryGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="M6.5 3.6h11M5 6.2h14" strokeWidth={1.8} />
      <rect x="3.6" y="8.6" width="16.8" height="12" rx="2.6" strokeWidth={2} />
      <path d="M10.8 17.6v-5.8l4.4-1v4.4" strokeWidth={1.6} />
      <circle cx="9.6" cy="17.7" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="14" cy="15.3" r="1.4" fill="currentColor" stroke="none" />
    </LineGlyph>
  )
}

export function CloseGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="m6 6 12 12M18 6 6 18" />
    </LineGlyph>
  )
}

/** The filter: three lines, shorter down. */
export function FilterGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="M4 7h16M7 12h10M10 17h4" />
    </LineGlyph>
  )
}

/** More: three dots. */
export function MoreGlyph({ size }: GlyphProps) {
  return (
    <Glyph size={size}>
      <circle cx="5.5" cy="12" r="1.5" />
      <circle cx="12" cy="12" r="1.5" />
      <circle cx="18.5" cy="12" r="1.5" />
    </Glyph>
  )
}

export function GridGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <rect x="4" y="4" width="6.5" height="6.5" rx="1.6" strokeWidth={1.8} />
      <rect x="13.5" y="4" width="6.5" height="6.5" rx="1.6" strokeWidth={1.8} />
      <rect x="4" y="13.5" width="6.5" height="6.5" rx="1.6" strokeWidth={1.8} />
      <rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.6" strokeWidth={1.8} />
    </LineGlyph>
  )
}

export function ListGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="M9 6.5h11M9 12h11M9 17.5h11" strokeWidth={1.8} />
      <circle cx="4.6" cy="6.5" r="0.5" fill="currentColor" />
      <circle cx="4.6" cy="12" r="0.5" fill="currentColor" />
      <circle cx="4.6" cy="17.5" r="0.5" fill="currentColor" />
    </LineGlyph>
  )
}

/** All of a list: lines and a note. */
export function AllGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="M3.5 7h9M3.5 12h7M3.5 17h5" strokeWidth={1.8} />
      <path d="M15.5 18V7.5l4.5-1.2" strokeWidth={1.8} />
      <circle cx="14" cy="18" r="1.6" fill="currentColor" stroke="none" />
    </LineGlyph>
  )
}

/** An account without a picture: a person. */
export function PersonGlyph({ size }: GlyphProps) {
  return (
    <Glyph size={size}>
      <circle cx="12" cy="8.2" r="3.6" />
      <path d="M4.8 19.2c0-3.6 3.2-6 7.2-6s7.2 2.4 7.2 6a.9.9 0 0 1-.9.9H5.7a.9.9 0 0 1-.9-.9Z" />
    </Glyph>
  )
}

export function SearchGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <circle cx="10.5" cy="10.5" r="6.3" />
      <path d="m15.3 15.3 5 5" />
    </LineGlyph>
  )
}

export function ChevronRightGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="m9.5 5 7 7-7 7" />
    </LineGlyph>
  )
}

export function ChevronDownGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="m5 9.5 7 7 7-7" />
    </LineGlyph>
  )
}

/** A favourite: Apple Music's star, filled once it is one. */
export function StarGlyph({ size, on = false }: GlyphProps & { on?: boolean }) {
  const star = 'M12 3.6l2.5 5.3 5.8.7-4.3 4 1.1 5.7L12 16.5l-5.1 2.8L8 13.6l-4.3-4 5.8-.7Z'
  return on ? (
    <Glyph size={size}>
      <path d={star} />
    </Glyph>
  ) : (
    <LineGlyph size={size}>
      <path d={star} strokeWidth={1.9} />
    </LineGlyph>
  )
}

/** Lyrics: a speech bubble, quotes in it. */
export function LyricsGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path
        d="M6.5 4h11A3.5 3.5 0 0 1 21 7.5v6a3.5 3.5 0 0 1-3.5 3.5H12l-4.6 3.4V17h-.9A3.5 3.5 0 0 1 3 13.5v-6A3.5 3.5 0 0 1 6.5 4Z"
        strokeWidth={1.9}
      />
      <path d="M10.3 8.2c-.9.7-1.4 1.7-1.5 3M15.3 8.2c-.9.7-1.4 1.7-1.5 3" strokeWidth={2} />
    </LineGlyph>
  )
}

export function PlusGlyph({ size }: GlyphProps) {
  return (
    <LineGlyph size={size}>
      <path d="M12 5v14M5 12h14" />
    </LineGlyph>
  )
}

/** The player's volume: a speaker, its waves as loud as it is (none: muted). */
export function SpeakerGlyph({ size, volume }: GlyphProps & { volume: number }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden>
      <path
        d="M3.5 9.4h3.2L11 5.6v12.8l-4.3-3.8H3.5Z"
        fill="currentColor"
        stroke="currentColor"
        strokeWidth={1.4}
        strokeLinejoin="round"
      />
      <g fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
        {volume === 0 ? (
          <path d="m15 9.5 5 5M20 9.5l-5 5" />
        ) : (
          <>
            <path d="M14.4 9.2a4 4 0 0 1 0 5.6" />
            {volume > 0.33 && <path d="M16.9 6.8a7.4 7.4 0 0 1 0 10.4" />}
            {volume > 0.66 && <path d="M19.4 4.4a10.8 10.8 0 0 1 0 15.2" />}
          </>
        )}
      </g>
    </svg>
  )
}
