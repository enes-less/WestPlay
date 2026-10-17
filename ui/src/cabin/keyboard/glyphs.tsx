import type { Shift } from './editing'

/** The keyboard's symbols, drawn for WestPlay (no SF Symbols): shift (outlined, filled, locked), delete,
 * the globe and hide. */

export function ShiftGlyph({ shift }: { shift: Shift }) {
  const filled = shift !== 'off'
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d={shift === 'locked' ? 'M12 3 3.5 11.5H8V16h8v-4.5h4.5Z' : 'M12 3.5 3.5 12.5H8V20h8v-7.5h4.5Z'}
        fill={filled ? 'currentColor' : 'none'}
        stroke="currentColor"
        strokeWidth="1.9"
        strokeLinejoin="round"
      />
      {shift === 'locked' && <rect x="8" y="18.5" width="8" height="2.2" rx="1" fill="currentColor" />}
    </svg>
  )
}

export function DeleteGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.9">
      <path d="M8.5 5H20a1.5 1.5 0 0 1 1.5 1.5v11A1.5 1.5 0 0 1 20 19H8.5L2.5 12Z" strokeLinejoin="round" />
      <path d="m11 9 6 6m0-6-6 6" strokeLinecap="round" />
    </svg>
  )
}

export function GlobeGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.8">
      <circle cx="12" cy="12" r="8.5" />
      <path d="M3.5 12h17M12 3.5c3 3.2 3 13.8 0 17M12 3.5c-3 3.2-3 13.8 0 17" />
    </svg>
  )
}

export function HideGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.8">
      <rect x="3" y="4" width="18" height="11" rx="2" />
      <path d="M6.5 7.5h1m2.5 0h1m2.5 0h1m2.5 0h.5m-10.5 3.5h11" strokeLinecap="round" />
      <path d="m9 18 3 2.5 3-2.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function SearchGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2.4">
      <circle cx="10.5" cy="10.5" r="6.5" />
      <path d="m15.5 15.5 5.5 5.5" strokeLinecap="round" />
    </svg>
  )
}

export function KeysGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.8">
      <rect x="2" y="6" width="20" height="12" rx="2.5" />
      <path d="M5.5 9.5h1m2.5 0h1m2.5 0h1m2.5 0h2.5m-13 3h1m2.5 0h1m2.5 0h1m2.5 0h1M8 15.5h8" strokeLinecap="round" />
    </svg>
  )
}
