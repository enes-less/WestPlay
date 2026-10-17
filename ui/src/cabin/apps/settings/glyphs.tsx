import type { ReactNode } from 'react'

/**
 * The Settings app's glyphs, our own drawings: white on each row's coloured tile (Apple's look, not Apple's
 * symbols), and the list's chevrons.
 */

function TileGlyph({ children }: { children: ReactNode }) {
  return (
    <svg viewBox="0 0 24 24" width="70%" height="70%" fill="#fff" aria-hidden>
      {children}
    </svg>
  )
}

/** Appearance: a circle half dark, half light. */
export function AppearanceGlyph() {
  return (
    <TileGlyph>
      <path d="M12 3.2a8.8 8.8 0 1 0 0 17.6 8.8 8.8 0 0 0 0-17.6Zm0 2a6.8 6.8 0 0 1 0 13.6Z" fillRule="evenodd" />
    </TileGlyph>
  )
}

/** Wallpaper: a flower of six petals. */
export function WallpaperGlyph() {
  return (
    <TileGlyph>
      {[0, 60, 120, 180, 240, 300].map((angle) => (
        <ellipse key={angle} cx="12" cy="6.9" rx="2.9" ry="4.1" transform={`rotate(${angle} 12 12)`} opacity="0.92" />
      ))}
      <circle cx="12" cy="12" r="2.4" fill="rgb(0 0 0 / 0.18)" />
    </TileGlyph>
  )
}

/** Dashboard: its widgets, a tall one beside two small ones. */
export function DashboardGlyph() {
  return (
    <TileGlyph>
      <rect x="3.5" y="4" width="8.2" height="16" rx="2.2" />
      <rect x="13.3" y="4" width="7.2" height="7.2" rx="2.2" />
      <rect x="13.3" y="12.8" width="7.2" height="7.2" rx="2.2" />
    </TileGlyph>
  )
}

/** Now Playing: a pair of beamed notes. */
export function NowPlayingGlyph() {
  return (
    <TileGlyph>
      <path d="M19.6 3.4v11.9a3 3 0 1 1-1.8-2.75V7.9l-8.2 1.9v7.5a3 3 0 1 1-1.8-2.75V6.5a1 1 0 0 1 .78-.98l9.8-2.27a1 1 0 0 1 1.22.98Z" />
    </TileGlyph>
  )
}

/** The chevron after a row that opens a page. */
export function ChevronGlyph({ back = false }: { back?: boolean }) {
  return (
    <svg
      viewBox="0 0 12 20"
      width="12"
      height="20"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      style={back ? { transform: 'scaleX(-1)' } : undefined}
      aria-hidden
    >
      <path d="m3 2.5 7 7.5-7 7.5" />
    </svg>
  )
}

/** The tick on a chosen option. */
export function CheckGlyph() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="20"
      height="20"
      fill="none"
      stroke="currentColor"
      strokeWidth="3"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="m5 12.5 4.6 4.6L19 7.5" />
    </svg>
  )
}
