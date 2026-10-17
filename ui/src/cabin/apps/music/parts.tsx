import { type CSSProperties, type ReactNode, useState } from 'react'
import { useText } from '../../state/text'
import {
  type AppleMusicItem,
  type AppleMusicSection,
  type AppleMusicTrack,
  currentTrack,
  openItem,
  openView,
  playFrom,
  useAppleMusic,
} from './appleMusic'
import { ChevronRightGlyph, NoteGlyph, RadioGlyph } from './glyphs'
import { type Covered, trackCover } from './look'

/** The pieces the Music app's pages share: covers, cards and their shelves, song rows. */

/**
 * A cover: Apple Music's picture, or (the preview's, which has none) one drawn in the cover's colour: a note
 * for an album or song, the name for a playlist or a category, a mast for a station, an artist's initials in
 * a circle.
 */
export function Cover({ item, className = '' }: { item: Covered; className?: string }) {
  const round = item.kind === 'artist'
  const classes = `am-cover${round ? ' round' : ''} ${className}`
  if (item.image) {
    return (
      <span className={classes} style={{ '--cover': item.color } as CSSProperties}>
        <img src={item.image} alt="" draggable={false} />
      </span>
    )
  }
  const style = { '--cover': item.color } as CSSProperties
  if (item.kind === 'artist') {
    const initials = item.name
      .split(/\s+/)
      .filter((word) => /^\p{L}/u.test(word))
      .slice(0, 2)
      .map((word) => word[0])
      .join('')
    return (
      <span className={`${classes} drawn`} style={style}>
        <b className="am-initials">{initials}</b>
      </span>
    )
  }
  if (item.kind === 'playlist' || item.kind === 'curator') {
    return (
      <span className={`${classes} drawn named`} style={style}>
        <b>{item.name}</b>
      </span>
    )
  }
  return (
    <span className={`${classes} drawn`} style={style}>
      {item.kind === 'station' ? <RadioGlyph size={44} /> : <NoteGlyph size={44} />}
    </span>
  )
}

/** The "E" of an explicit song or album. */
export function Explicit() {
  const text = useText()
  return (
    <span className="am-explicit" role="img" aria-label={text.appleMusic.explicit}>
      E
    </span>
  )
}

/** A card on a shelf: the cover, its name and who it's by (an artist's: a round picture, the name under). */
export function Tile({ item }: { item: AppleMusicItem }) {
  return (
    <button
      type="button"
      className={item.kind === 'artist' ? 'am-tile artist' : 'am-tile'}
      onClick={() => openItem(item)}
    >
      <Cover item={item} />
      <span className="am-tile-name">
        {item.name}
        {item.explicit && <Explicit />}
      </span>
      {item.kind !== 'artist' && item.by && <span className="am-tile-by">{item.by}</span>}
    </button>
  )
}

/** A big card (New's and Radio's first): its label over its name and who it's by, then the editors' wide
 * picture with their note on it. */
export function HeroCard({ item }: { item: AppleMusicItem }) {
  const text = useText()
  // A picture that doesn't load: the cover on its colour instead.
  const [broken, setBroken] = useState(false)
  return (
    <button type="button" className="am-hero" onClick={() => openItem(item)}>
      <span className="am-hero-label">{item.live ? text.appleMusic.live : (item.label ?? '')}</span>
      <span className="am-hero-name">{item.name}</span>
      <span className="am-hero-by">{item.kind === 'station' ? text.appleMusic.kinds.station : item.by}</span>
      <span className="am-hero-art" style={{ '--cover': item.color } as CSSProperties}>
        {item.wide && !broken ? (
          <img src={item.wide} alt="" draggable={false} onError={() => setBroken(true)} />
        ) : (
          <Cover item={item} className="am-hero-cover" />
        )}
        {item.note && <span className="am-hero-note">{item.note}</span>}
      </span>
    </button>
  )
}

/** A section's title: with See All (a chevron after it), when there's more than shows. */
export function SectionHead({ title, onAll }: { title: string; onAll?(): void }) {
  if (!title) return null
  return onAll ? (
    <button type="button" className="am-section-head" onClick={onAll}>
      <h2>{title}</h2>
      <ChevronRightGlyph size={30} />
    </button>
  ) : (
    <div className="am-section-head">
      <h2>{title}</h2>
    </div>
  )
}

/** How many cards a shelf shows before its See All. */
const SHELF_MOST = 12

/** A section of a page: big cards, a shelf of covers (what was played lately, bigger), or songs in columns. */
export function Section({ section, title }: { section: AppleMusicSection; title?: string }) {
  const name = title ?? section.title
  const more = section.items.length > SHELF_MOST || section.songs.length > 12
  const onAll = more ? () => openView({ kind: 'section', section: { ...section, title: name } }) : undefined
  if (section.style === 'hero') {
    return (
      <section className="am-section">
        <SectionHead title={name} />
        <div className="am-shelf am-heroes">
          {section.items.map((item, i) => (
            <HeroCard key={`${item.id}:${i}`} item={item} />
          ))}
        </div>
      </section>
    )
  }
  if (section.style === 'songs') {
    return (
      <section className="am-section">
        <SectionHead title={name} onAll={onAll} />
        <div className="am-shelf am-song-columns">
          {section.songs.slice(0, 12).map((track, i) => (
            <SongRow key={`${track.id}:${i}`} track={track} onPlay={() => playSongs(section.songs, i)} cover />
          ))}
        </div>
      </section>
    )
  }
  return (
    <section className="am-section">
      <SectionHead title={name} onAll={onAll} />
      <div className={section.style === 'recent' ? 'am-shelf am-recent' : 'am-shelf'}>
        {section.items.slice(0, SHELF_MOST).map((item, i) => (
          <Tile key={`${item.id}:${i}`} item={item} />
        ))}
      </div>
    </section>
  )
}

/** Songs of a list play as that list. */
function playSongs(songs: AppleMusicTrack[], index: number): void {
  playFrom(null, songs, index)
}

/** Whether a song is the one playing (from where it's listed: `contextId`, or anywhere). */
function usePlayingSong(track: AppleMusicTrack, contextId: string | null): 'playing' | 'paused' | null {
  return useAppleMusic((state) => {
    const playback = state.playback
    if (!playback || (contextId && playback.context?.id !== contextId)) return null
    if (currentTrack(playback)?.id !== track.id) return null
    return playback.playing ? 'playing' : 'paused'
  })
}

/** Apple Music's bars, beside the song playing (still while it's paused). */
export function Bars({ playing }: { playing: boolean }) {
  return (
    <span className={playing ? 'am-bars playing' : 'am-bars'} aria-hidden>
      <i />
      <i />
      <i />
      <i />
    </span>
  )
}

/** A song in a list: its number on its album (or its cover), its name and artist, its length; the bars
 * while it plays. Pressed, it plays from there. */
export function SongRow({
  track,
  onPlay,
  cover = false,
  number = null,
  contextId = null,
  end,
}: {
  track: AppleMusicTrack
  onPlay(): void
  cover?: boolean
  number?: number | null
  contextId?: string | null
  end?: ReactNode
}) {
  const state = usePlayingSong(track, contextId)
  return (
    <button type="button" className={state ? 'am-song am-current' : 'am-song'} onClick={onPlay}>
      {cover ? (
        <span className="am-song-cover">
          <Cover item={trackCover(track)} />
          {state && <Bars playing={state === 'playing'} />}
        </span>
      ) : (
        <span className="am-song-number">{state ? <Bars playing={state === 'playing'} /> : number}</span>
      )}
      <span className="am-song-text">
        <span className="am-song-name">
          {track.name}
          {track.explicit && <Explicit />}
        </span>
        {(cover || !number) && <span className="am-song-by">{track.artist}</span>}
      </span>
      {end}
    </button>
  )
}

/** A page that didn't come: Apple Music didn't answer. */
export function Failed({ onRetry }: { onRetry(): void }) {
  const text = useText()
  return (
    <div className="am-failed">
      <p>{text.appleMusic.failed}</p>
      <button type="button" className="am-pill" onClick={onRetry}>
        {text.appleMusic.tryAgain}
      </button>
    </div>
  )
}

/** A page coming: Apple's spinner. */
export function Loading() {
  return (
    <div className="am-loading">
      <span className="am-spinner" aria-hidden />
    </div>
  )
}
