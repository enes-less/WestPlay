/** The Spotify app's symbols, drawn for WestPlay in the manner of Spotify's own (24-unit grid, filled, round
 * ends), a little heavier for the truck's screen. */

/** Home: the house, filled where it is. */
export function HomeGlyph({ on }: { on: boolean }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      {on ? (
        <path d="M12 2.6 2.8 10v11.4h6.6v-6.6h5.2v6.6h6.6V10z" fill="currentColor" />
      ) : (
        <path
          d="M12 3.8 3.9 10.3v10h4.7v-6.6h6.8v6.6h4.7v-10z"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.1"
          strokeLinejoin="round"
        />
      )}
    </svg>
  )
}

/** Your Library: books on a shelf, the last one leaning. */
export function LibraryGlyph({ on }: { on: boolean }) {
  const width = on ? 2.9 : 2.1
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M4.5 3v18M10 3v18M15 3.6l5 16.8"
        fill="none"
        stroke="currentColor"
        strokeWidth={width}
        strokeLinecap="round"
      />
    </svg>
  )
}

export function SearchGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="10.5" cy="10.5" r="6.8" fill="none" stroke="currentColor" strokeWidth="2.3" />
      <path d="m15.6 15.6 5.4 5.4" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" />
    </svg>
  )
}

/** Browse all, at the search pill's end: a tray with a card in it. */
export function BrowseGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M3.5 12.5 6 5h12l2.5 7.5V19a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 19z M3.5 12.5h5l1 2.5h5l1-2.5h5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinejoin="round"
      />
    </svg>
  )
}

export function PlayGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M7 4.3v15.4a1 1 0 0 0 1.5.86l12.6-7.7a1 1 0 0 0 0-1.72L8.5 3.44A1 1 0 0 0 7 4.3z" fill="currentColor" />
    </svg>
  )
}

export function PauseGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <rect x="5.5" y="3.5" width="4.4" height="17" rx="1.2" fill="currentColor" />
      <rect x="14.1" y="3.5" width="4.4" height="17" rx="1.2" fill="currentColor" />
    </svg>
  )
}

export function NextGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M4.5 4.6v14.8a.9.9 0 0 0 1.4.75l11-7.4a.9.9 0 0 0 0-1.5l-11-7.4a.9.9 0 0 0-1.4.75z"
        fill="currentColor"
      />
      <rect x="17.2" y="3.6" width="2.8" height="16.8" rx="1.2" fill="currentColor" />
    </svg>
  )
}

export function PreviousGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" style={{ scale: '-1 1' }}>
      <path
        d="M4.5 4.6v14.8a.9.9 0 0 0 1.4.75l11-7.4a.9.9 0 0 0 0-1.5l-11-7.4a.9.9 0 0 0-1.4.75z"
        fill="currentColor"
      />
      <rect x="17.2" y="3.6" width="2.8" height="16.8" rx="1.2" fill="currentColor" />
    </svg>
  )
}

/** Shuffle: two paths crossing, each arrowed. */
export function ShuffleGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M2.5 6.5h3.2c2 0 3.2 1 4.3 2.7l3.9 5.6c1.1 1.7 2.3 2.7 4.3 2.7h2.6M2.5 17.5h3.2c1.4 0 2.4-.5 3.2-1.4M14.6 7.9c.8-.9 1.8-1.4 3.2-1.4h2.6M17.8 3.6l2.9 2.9-2.9 2.9M17.8 14.6l2.9 2.9-2.9 2.9"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.1"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/** Repeat: a loop, arrowed; `one` adds the 1 of repeating one song. */
export function RepeatGlyph({ one }: { one: boolean }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M6.5 18.5H6A3.5 3.5 0 0 1 2.5 15V9A3.5 3.5 0 0 1 6 5.5h12A3.5 3.5 0 0 1 21.5 9v6a3.5 3.5 0 0 1-3.5 3.5h-6.5M14.2 15.8l-2.7 2.7 2.7 2.7"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.1"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      {one && (
        <>
          <circle cx="12" cy="12" r="4.4" fill="currentColor" />
          <path d="M11.2 10.6 12.4 9.8v4.6" fill="none" stroke="#000" strokeWidth="1.5" strokeLinecap="round" />
        </>
      )}
    </svg>
  )
}

/** Add to Liked Songs: a plus in a ring; added, a check in a green disc. */
export function AddGlyph({ added }: { added: boolean }) {
  return added ? (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="10" fill="var(--sp-green)" />
      <path
        d="m7.3 12.3 3.1 3.1 6.3-6.6"
        fill="none"
        stroke="#000"
        strokeWidth="2.3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  ) : (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="9.2" fill="none" stroke="currentColor" strokeWidth="1.9" />
      <path d="M12 7.6v8.8M7.6 12h8.8" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  )
}

export function DownloadGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="9.2" fill="none" stroke="currentColor" strokeWidth="1.9" />
      <path
        d="M12 7.2v8.6M8.4 12.4 12 16l3.6-3.6"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

export function MoreGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="4.5" cy="12" r="2.1" fill="currentColor" />
      <circle cx="12" cy="12" r="2.1" fill="currentColor" />
      <circle cx="19.5" cy="12" r="2.1" fill="currentColor" />
    </svg>
  )
}

export function BackGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M15 4.5 7.5 12l7.5 7.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/** Closes Now Playing (it goes back down). */
export function DownGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="m4.5 8.5 7.5 7.5 7.5-7.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/** The queue: lines, the first one playing. */
export function QueueGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M3 4.5 8 7.5 3 10.5z" fill="currentColor" />
      <path d="M11 7.5h10M3 13.5h18M3 19h18" stroke="currentColor" strokeWidth="2.1" strokeLinecap="round" />
    </svg>
  )
}

/** Lyrics: a microphone. */
export function LyricsGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M12 3a3.3 3.3 0 0 1 3.3 3.3v5a3.3 3.3 0 0 1-6.6 0v-5A3.3 3.3 0 0 1 12 3ZM5.8 11.2a6.2 6.2 0 0 0 12.4 0M12 17.4V21"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.1"
        strokeLinecap="round"
      />
    </svg>
  )
}

/** Pinned in Your Library. */
export function PinGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M15.8 2.6 21.4 8.2l-2.3.6-3.9 3.9.3 4.6-1.6 1.6-3.9-3.9-5 5-1.1-1.1 5-5-3.9-3.9 1.6-1.6 4.6.3 3.9-3.9z"
        fill="currentColor"
      />
    </svg>
  )
}

/** Downloaded (in Your Library's rows): an arrow in a green disc. */
export function DownloadedGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="10" fill="currentColor" />
      <path
        d="M12 6.8v9M8.3 12.2 12 15.9l3.7-3.7"
        fill="none"
        stroke="#000"
        strokeWidth="2.3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/** Your Library's view: a grid of covers, or a list. */
export function GridGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <rect x="3.5" y="3.5" width="7" height="7" rx="1.4" fill="none" stroke="currentColor" strokeWidth="2" />
      <rect x="13.5" y="3.5" width="7" height="7" rx="1.4" fill="none" stroke="currentColor" strokeWidth="2" />
      <rect x="3.5" y="13.5" width="7" height="7" rx="1.4" fill="none" stroke="currentColor" strokeWidth="2" />
      <rect x="13.5" y="13.5" width="7" height="7" rx="1.4" fill="none" stroke="currentColor" strokeWidth="2" />
    </svg>
  )
}

export function ListGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 6h16M4 12h16M4 18h16" stroke="currentColor" strokeWidth="2.1" strokeLinecap="round" />
    </svg>
  )
}

/** Your Library's order: arrows up and down. */
export function SortGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M7.5 20V4M3.8 7.7 7.5 4l3.7 3.7M16.5 4v16M12.8 16.3l3.7 3.7 3.7-3.7"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/** A verified artist: a blue rosette with a check. */
export function VerifiedGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M12 1.8 14.6 3.6l3.1-.2 1 3 2.6 1.8-1 3 1 3-2.6 1.8-1 3-3.1-.2L12 22.2l-2.6-1.8-3.1.2-1-3-2.6-1.8 1-3-1-3 2.6-1.8 1-3 3.1.2z"
        fill="#4cb3ff"
      />
      <path
        d="m7.8 12.2 2.8 2.8 5.6-5.8"
        fill="none"
        stroke="#fff"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/** Liked Songs' cover: the white heart. */
export function HeartGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M12 20.6s-8.6-5.1-8.6-11.2A4.9 4.9 0 0 1 12 6.4a4.9 4.9 0 0 1 8.6 3c0 6.1-8.6 11.2-8.6 11.2z"
        fill="currentColor"
      />
    </svg>
  )
}

export function CloseGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="m6.5 6.5 11 11M17.5 6.5l-11 11" stroke="currentColor" strokeWidth="2.3" strokeLinecap="round" />
    </svg>
  )
}

/** Playing: Spotify's green bars, moving while it plays. */
export function Equalizer({ playing }: { playing: boolean }) {
  return (
    <span className={playing ? 'sp-eq playing' : 'sp-eq'} aria-hidden="true">
      <i />
      <i />
      <i />
      <i />
    </span>
  )
}

/** The account, without its picture (WestPlay shows none of it): a person, as Spotify's own fallback. */
export function PersonGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="7.8" r="4.3" fill="none" stroke="currentColor" strokeWidth="2" />
      <path
        d="M3.8 21c.6-4.3 3.9-7 8.2-7s7.6 2.7 8.2 7"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  )
}

/** The volume: a speaker, its waves as many as it's loud (none, a cross, muted). */
export function VolumeGlyph({ volume }: { volume: number }) {
  const waves = volume === 0 ? 0 : volume < 0.34 ? 1 : volume < 0.67 ? 2 : 3
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M3 9.2h3.6L11.5 5v14l-4.9-4.2H3z" fill="currentColor" strokeLinejoin="round" />
      {waves === 0 && <path d="m15.5 9.5 5 5m0-5-5 5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />}
      {waves >= 1 && (
        <path
          d="M14.5 9.6a3.4 3.4 0 0 1 0 4.8"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
        />
      )}
      {waves >= 2 && (
        <path
          d="M16.9 7.3a6.6 6.6 0 0 1 0 9.4"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
        />
      )}
      {waves >= 3 && (
        <path d="M19.3 5a9.8 9.8 0 0 1 0 14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      )}
    </svg>
  )
}
