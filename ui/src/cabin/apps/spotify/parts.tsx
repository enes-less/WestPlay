import type { CSSProperties, ReactNode } from 'react'
import { useText } from '../../state/text'
import { Equalizer, HeartGlyph, PinGlyph, DownloadedGlyph } from './glyphs'
import { clock, type Covered, trackCover, useKindLine, usePlaysFrom } from './look'
import { currentTrack, type SpotifyItem, type SpotifyTrack, useSpotify } from './spotify'

/** The pieces the Spotify app's pages share: covers, cards and their shelves, rows. */

/**
 * A cover: the picture Spotify has, or (the preview's, which has none) one drawn in the cover's colour as
 * Spotify draws its own: Liked Songs' heart on its violet, a playlist's name on its colour, an album's
 * shapes, an artist's initials in a circle.
 */
export function Cover({ item, className = '' }: { item: Covered; className?: string }) {
  const round = item.kind === 'artist'
  const classes = `sp-cover${round ? ' round' : ''} ${className}`
  if (item.image) {
    return (
      <span className={classes}>
        <img src={item.image} alt="" draggable={false} />
      </span>
    )
  }
  const style = { '--cover': item.color } as CSSProperties
  if (item.kind === 'liked') {
    return (
      <span className={`${classes} drawn liked`}>
        <HeartGlyph />
      </span>
    )
  }
  if (item.kind === 'artist') {
    const initials = item.name
      .split(/\s+/)
      .filter((word) => /^\p{L}/u.test(word))
      .slice(0, 2)
      .map((word) => word[0])
      .join('')
    return (
      <span className={`${classes} drawn artist`} style={style}>
        <b>{initials}</b>
      </span>
    )
  }
  if (item.kind === 'album') {
    // Two shapes, placed by the name: every album its own.
    const seed = [...item.name].reduce((sum, letter) => (sum * 17 + letter.charCodeAt(0)) % 997, 3)
    const shapes = { '--x': `${20 + (seed % 60)}%`, '--y': `${25 + ((seed >> 2) % 50)}%` } as CSSProperties
    return <span className={`${classes} drawn album`} style={{ ...style, ...shapes }} />
  }
  return (
    <span className={`${classes} drawn ${item.kind}`} style={style}>
      <b>{item.name}</b>
    </span>
  )
}

/** A card on a shelf: the cover, its name, and its description or what it is. */
export function Card({ item, onOpen }: { item: SpotifyItem; onOpen(item: SpotifyItem): void }) {
  const kindLine = useKindLine()
  const { from } = usePlaysFrom(item.uri)
  return (
    <button type="button" className="sp-card" onClick={() => onOpen(item)}>
      <Cover item={item} />
      <span className={from ? 'sp-card-name sp-on' : 'sp-card-name'}>{item.name}</span>
      <span className="sp-card-sub">{item.description || kindLine(item)}</span>
    </button>
  )
}

/** A shelf: its title over a row of cards, more to the side. */
export function Shelf({
  title,
  items,
  onOpen,
  action,
}: {
  title: string
  items: SpotifyItem[]
  onOpen(item: SpotifyItem): void
  action?: ReactNode
}) {
  if (items.length === 0) return null
  return (
    <section className="sp-section">
      <div className="sp-section-head">
        <h3>{title}</h3>
        {action}
      </div>
      <div className="sp-shelf">
        {items.map((item) => (
          <Card key={item.uri} item={item} onOpen={onOpen} />
        ))}
      </div>
    </section>
  )
}

/** A row of Your Library or the searches: the cover, the name, what it is (pinned, downloaded). */
export function ItemRow({
  item,
  onOpen,
  end,
}: {
  item: SpotifyItem
  onOpen(item: SpotifyItem): void
  end?: ReactNode
}) {
  const kindLine = useKindLine()
  const { from } = usePlaysFrom(item.uri)
  return (
    <div className="sp-row">
      <button type="button" className="sp-row-open" onClick={() => onOpen(item)}>
        <Cover item={item} />
        <span className="sp-row-text">
          <span className={from ? 'sp-row-name sp-on' : 'sp-row-name'}>{item.name}</span>
          <span className="sp-row-sub">
            {item.pinned && <PinGlyph />}
            {item.downloaded && <DownloadedGlyph />}
            <span>{kindLine(item)}</span>
          </span>
        </span>
      </button>
      {end}
    </div>
  )
}

/** A song in a list: its number (the green bars while it plays), its cover (in a playlist), its name and
 * artists, its length. Pressed, it plays from there. */
export function TrackRow({
  track,
  number,
  contextUri,
  cover,
  onPlay,
}: {
  track: SpotifyTrack
  number: number | null
  contextUri: string
  cover: boolean
  onPlay(): void
}) {
  const text = useText()
  const { current, playing } = useSpotify((state) => {
    const now = state.playback?.context?.uri === contextUri ? currentTrack(state.playback) : null
    return now?.uri === track.uri ? (state.playback!.playing ? PLAYING_TRACK : CURRENT) : OTHER
  })
  return (
    <button type="button" className={current ? 'sp-track sp-current' : 'sp-track'} onClick={onPlay}>
      {number !== null && <span className="sp-track-number">{current ? <Equalizer playing={playing} /> : number}</span>}
      {cover && <Cover item={trackCover(track)} />}
      <span className="sp-track-text">
        <span className="sp-track-name">{track.name}</span>
        <span className="sp-track-artists">
          {track.explicit && <span className="sp-explicit">{text.spotify.explicit}</span>}
          {track.artists.map((artist) => artist.name).join(', ')}
        </span>
      </span>
      {number === null && current && <Equalizer playing={playing} />}
      <span className="sp-track-time">{clock(track.duration)}</span>
    </button>
  )
}

const PLAYING_TRACK = { current: true, playing: true }
const CURRENT = { current: true, playing: false }
const OTHER = { current: false, playing: false }

/** Spotify's chips: the one chosen green. */
export function Chips<T extends string>({
  options,
  chosen,
  onChoose,
}: {
  options: readonly { id: T; label: string }[]
  chosen: T | null
  onChoose(id: T): void
}) {
  return (
    <div className="sp-chips">
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          className={chosen === option.id ? 'sp-chip on' : 'sp-chip'}
          aria-pressed={chosen === option.id}
          onClick={() => onChoose(option.id)}
        >
          {option.label}
        </button>
      ))}
    </div>
  )
}

/** A line of words still coming. */
function Line({ short = false }: { short?: boolean }) {
  return <span className={short ? 'sp-skel-line short' : 'sp-skel-line'} />
}

/** Home's tiles still coming: their shapes, grey, breathing (as Spotify waits). */
export function ShortcutsSkeleton() {
  return (
    <div className="sp-shortcuts sp-skeleton" role="progressbar">
      {Array.from({ length: 8 }, (_, index) => (
        <span key={index} className="sp-shortcut">
          <span className="sp-cover sp-skel" />
          <Line />
        </span>
      ))}
    </div>
  )
}

/** A shelf still coming: its title and a row of cards. */
export function ShelfSkeleton({ round = false }: { round?: boolean }) {
  return (
    <section className="sp-section sp-skeleton" role="progressbar">
      <div className="sp-section-head">
        <span className="sp-skel-line title" />
      </div>
      <div className="sp-shelf">
        {Array.from({ length: 6 }, (_, index) => (
          <span key={index} className="sp-card">
            <span className={round ? 'sp-cover round sp-skel' : 'sp-cover sp-skel'} />
            <Line />
            <Line short />
          </span>
        ))}
      </div>
    </section>
  )
}

/** Rows still coming (Your Library's, a search's). */
export function RowsSkeleton({ count = 8 }: { count?: number }) {
  return (
    <div className="sp-rows sp-skeleton" role="progressbar">
      {Array.from({ length: count }, (_, index) => (
        <div key={index} className="sp-row">
          <span className="sp-row-open">
            <span className="sp-cover sp-skel" />
            <span className="sp-row-text">
              <Line />
              <Line short />
            </span>
          </span>
        </div>
      ))}
    </div>
  )
}

/** A playlist's, album's or artist's page still coming: its header, then its songs. */
export function PageSkeleton({ round = false }: { round?: boolean }) {
  return (
    <div className="sp-scroll sp-page sp-skeleton" role="progressbar">
      <header className={round ? 'sp-header sp-artist-header' : 'sp-header'}>
        <span className={round ? 'sp-cover round sp-header-cover sp-skel' : 'sp-cover sp-header-cover sp-skel'} />
        <span className="sp-header-text">
          <Line short />
          <span className="sp-skel-line big" />
          <Line short />
        </span>
      </header>
      <div className="sp-tracks">
        {Array.from({ length: 8 }, (_, index) => (
          <span key={index} className="sp-track">
            <span className="sp-track-text">
              <Line />
              <Line short />
            </span>
          </span>
        ))}
      </div>
    </div>
  )
}

/** Browse all's tiles still coming. */
export function BrowseSkeleton() {
  return (
    <div className="sp-browse sp-skeleton" role="progressbar">
      {Array.from({ length: 12 }, (_, index) => (
        <span key={index} className="sp-tile sp-skel" />
      ))}
    </div>
  )
}
