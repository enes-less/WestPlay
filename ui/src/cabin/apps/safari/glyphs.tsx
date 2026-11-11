/** Safari's toolbar symbols, drawn in-house after SF Symbols' shapes (stroked, round caps, on a 24 grid). */

function Glyph({ children, className = 'sf-glyph' }: { children: React.ReactNode; className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  )
}

export const BackGlyph = () => (
  <Glyph>
    <path d="M15 4.5 7.5 12l7.5 7.5" />
  </Glyph>
)

export const ForwardGlyph = () => (
  <Glyph>
    <path d="m9 4.5 7.5 7.5L9 19.5" />
  </Glyph>
)

/** sidebar.left */
export const SidebarGlyph = () => (
  <Glyph>
    <rect x="2.5" y="4" width="19" height="16" rx="3.5" />
    <path d="M9 4v16M4.8 8h2M4.8 11h2" />
  </Glyph>
)

export const PlusGlyph = () => (
  <Glyph>
    <path d="M12 5v14M5 12h14" />
  </Glyph>
)

/** square.on.square: the tab overview. */
export const TabsGlyph = () => (
  <Glyph>
    <rect x="7.5" y="7.5" width="13" height="13" rx="3" />
    <path d="M16.5 4.5V4a1.5 1.5 0 0 0-1.5-1.5H6A3.5 3.5 0 0 0 2.5 6v9A1.5 1.5 0 0 0 4 16.5h.5" />
  </Glyph>
)

export const ReloadGlyph = () => (
  <Glyph>
    <path d="M19 12a7 7 0 1 1-2.05-4.95" />
    <path d="M17.5 3.5v4h-4" />
  </Glyph>
)

export const CloseGlyph = () => (
  <Glyph>
    <path d="m6.5 6.5 11 11M17.5 6.5l-11 11" />
  </Glyph>
)

export const SearchGlyph = () => (
  <Glyph className="sf-glyph small">
    <circle cx="10.5" cy="10.5" r="6.5" />
    <path d="m15.5 15.5 5 5" />
  </Glyph>
)

/** The page menu (iPadOS 26's address field button). */
export const PageMenuGlyph = () => (
  <Glyph>
    <rect x="4" y="3" width="16" height="18" rx="3.5" />
    <path d="M8 8h8M8 12h8M8 16h5" />
  </Glyph>
)

export const WarningGlyph = () => (
  <Glyph className="sf-glyph small">
    <path d="M12 3.5 21.5 20h-19z" />
    <path d="M12 10v4.5M12 17.3v.2" />
  </Glyph>
)

/** The compass, Safari's own (its error page). */
export const CompassGlyph = () => (
  <svg className="sf-compass" viewBox="0 0 64 64" aria-hidden="true">
    <circle cx="32" cy="32" r="27" fill="none" stroke="currentColor" strokeWidth="4" />
    <path d="M44 20 35 35l-6-6z" fill="var(--apple-red)" />
    <path d="M20 44 29 29l6 6z" fill="currentColor" opacity="0.5" />
  </svg>
)

export const CheckGlyph = () => (
  <Glyph>
    <path d="m5 12.5 4.5 4.5L19 7.5" />
  </Glyph>
)

/** globe: a web address typed. */
export const GlobeGlyph = () => (
  <Glyph className="sf-glyph small">
    <circle cx="12" cy="12" r="8.5" />
    <path d="M3.5 12h17M12 3.5c2.6 2.4 3.8 5.2 3.8 8.5s-1.2 6.1-3.8 8.5c-2.6-2.4-3.8-5.2-3.8-8.5S9.4 5.9 12 3.5Z" />
  </Glyph>
)

/** ellipsis: More. */
export const MoreGlyph = () => (
  <svg className="sf-glyph" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <circle cx="5.5" cy="12" r="1.9" />
    <circle cx="12" cy="12" r="1.9" />
    <circle cx="18.5" cy="12" r="1.9" />
  </svg>
)
