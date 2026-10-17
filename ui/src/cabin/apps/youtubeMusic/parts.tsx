import type { CSSProperties, ReactNode } from 'react'
import { useText } from '../../state/text'
import { EqualizerGlyph, MoreGlyph, PlayGlyph } from './glyphs'
import {
  type YouTubeMusicItem,
  type YouTubeMusicSection,
  openItem,
  openView,
  play,
  useYouTubeMusic,
} from './youtubeMusic'

/** The pieces the YouTube Music app's pages share: covers, cards and shelves, song rows, Quick picks' columns,
 * mood buttons, words, and the shapes of what's still coming. */

/** A colour for something without a picture (the preview's): from its title, always the same. */
function hueOf(title: string): number {
  let hash = 0
  for (const char of title) hash = (hash * 31 + char.charCodeAt(0)) % 360
  return hash
}

/** A cover: YouTube Music's picture (square; a video's wide; an artist's round), or (the preview's, which have
 * none) one drawn in a colour of its title's, its initials on it. */
export function Cover({
  item,
  className = '',
}: {
  item: Pick<YouTubeMusicItem, 'title' | 'kind' | 'image'>
  className?: string
}) {
  const round = item.kind === 'artist' || item.kind === 'channel'
  const classes = `ytm-cover${round ? ' round' : ''} ${className}`
  if (item.image) {
    return (
      <span className={classes}>
        <img src={item.image} alt="" draggable={false} loading="lazy" />
      </span>
    )
  }
  const hue = hueOf(item.title)
  const initials = item.title
    .split(/\s+/)
    .filter((word) => /^\p{L}/u.test(word))
    .slice(0, 2)
    .map((word) => word[0])
    .join('')
  return (
    <span className={`${classes} drawn`} style={{ '--hue': hue } as CSSProperties}>
      <b>{initials}</b>
    </span>
  )
}

/** The "E" of an explicit song or album. */
export function Explicit() {
  return (
    <span className="ytm-explicit" aria-label="Explicit">
      E
    </span>
  )
}

/** Whether a song is the one playing now, and playing. */
function usePlaying(video: string | undefined): 'playing' | 'paused' | null {
  return useYouTubeMusic((state) => {
    const playback = state.playback
    if (!video || !playback || playback.track.video !== video) return null
    return playback.playing ? 'playing' : 'paused'
  })
}

/** A card: its square cover (a list's has its own Play over it; a video's picture cropped square, as YouTube
 * Music's iPad app shows it), its title and YouTube Music's line under it. */
export function Card({ item }: { item: YouTubeMusicItem }) {
  const list = (item.kind === 'album' || item.kind === 'playlist') && item.play
  return (
    <div
      role="button"
      tabIndex={0}
      className={item.kind === 'artist' ? 'ytm-card artist' : 'ytm-card'}
      onClick={() => openItem(item)}
    >
      <span className="ytm-card-cover">
        <Cover item={item} />
        {list && (
          <button
            type="button"
            className="ytm-card-play"
            aria-label={item.title}
            onClick={(event) => {
              event.stopPropagation()
              play(item)
            }}
          >
            <PlayGlyph size={30} />
          </button>
        )}
      </span>
      <span className="ytm-card-title">
        {item.explicit && <Explicit />}
        {item.title}
      </span>
      {item.subtitle && <span className="ytm-card-sub">{item.subtitle}</span>}
    </div>
  )
}

/** A song's row: its cover (or its number on its album; playing, the bars), its title and line, its ⋮ (its
 * menu: radio, queue, its album and artist), as YouTube Music's iPad app lists songs. */
export function SongRow({ item, number }: { item: YouTubeMusicItem; number?: string }) {
  const text = useText()
  const playing = usePlaying(item.video)
  const song = item.kind === 'song' || item.kind === 'video' || item.kind === 'episode'
  return (
    <div role="button" tabIndex={0} className={playing ? 'ytm-row now' : 'ytm-row'} onClick={() => openItem(item)}>
      {number !== undefined ? (
        <span className="ytm-row-number">{playing ? <EqualizerGlyph size={22} /> : number}</span>
      ) : (
        <span className="ytm-row-cover">
          <Cover item={item} />
          {playing && (
            <span className={`ytm-row-playing ${playing}`}>
              <EqualizerGlyph size={24} />
            </span>
          )}
        </span>
      )}
      <span className="ytm-row-text">
        <span className="ytm-row-title">{item.title}</span>
        <span className="ytm-row-sub">
          {item.explicit && <Explicit />}
          {item.subtitle}
        </span>
      </span>
      {song && (
        <button
          type="button"
          className="ytm-round small ytm-row-more"
          aria-label={text.youtubeMusic.moreActions}
          onClick={(event) => {
            event.stopPropagation()
            useYouTubeMusic.setState({ menu: item })
          }}
        >
          <MoreGlyph size={24} />
        </button>
      )}
    </div>
  )
}

/** A mood's or genre's button: its name, its colour down its side. */
export function MoodButton({ item }: { item: YouTubeMusicItem }) {
  return (
    <button
      type="button"
      className="ytm-mood"
      style={{ '--mood': item.color ?? 'transparent' } as CSSProperties}
      onClick={() => openItem(item)}
    >
      {item.title}
    </button>
  )
}

/** A shelf's head: the small line over its title, its title, and its "More". */
export function SectionHead({ section }: { section: YouTubeMusicSection }) {
  const text = useText()
  const more = section.more
  if (!section.title && !section.strapline) return null
  return (
    <header className="ytm-section-head">
      <span className="ytm-section-titles">
        {section.strapline && <span className="ytm-strapline">{section.strapline}</span>}
        {section.title && <h2>{section.title}</h2>}
      </span>
      {more && (
        <button
          type="button"
          className="ytm-pill outline"
          onClick={() => openView({ kind: 'browse', title: section.title, page: more })}
        >
          {text.youtubeMusic.more}
        </button>
      )}
    </header>
  )
}

/** A page's shelf, as YouTube Music lays it out: cards side by side, Quick picks' columns of four songs, songs
 * one under another, mood buttons, words. */
export function Section({ section }: { section: YouTubeMusicSection }) {
  let body: ReactNode
  switch (section.style) {
    case 'columns': {
      const columns: YouTubeMusicItem[][] = []
      section.items.forEach((item, index) => {
        if (index % 4 === 0) columns.push([])
        columns[columns.length - 1].push(item)
      })
      body = (
        <div className="ytm-columns">
          {columns.map((column, index) => (
            <div key={index} className="ytm-column">
              {column.map((item, at) => (
                <SongRow key={`${item.video ?? item.title}-${at}`} item={item} number={item.rank || undefined} />
              ))}
            </div>
          ))}
        </div>
      )
      break
    }
    case 'songs':
      body = (
        <div className="ytm-songs">
          {section.items.map((item, index) => (
            <SongRow key={`${item.video ?? item.title}-${index}`} item={item} number={item.rank || undefined} />
          ))}
        </div>
      )
      break
    case 'moods':
    case 'moodRows':
      body = (
        <div className={section.style === 'moods' ? 'ytm-moods' : 'ytm-mood-rows'}>
          {section.items.map((item, index) => (
            <MoodButton key={`${item.title}-${index}`} item={item} />
          ))}
        </div>
      )
      break
    case 'about':
      body = <p className="ytm-about">{section.text}</p>
      break
    default:
      body = (
        <div className="ytm-shelf">
          {section.items.map((item, index) => (
            <Card key={`${item.browse?.id ?? item.video ?? item.title}-${index}`} item={item} />
          ))}
        </div>
      )
  }
  return (
    <section className={`ytm-section ${section.style}`}>
      <SectionHead section={section} />
      {body}
    </section>
  )
}

/** A page's shelves still coming: their shapes, grey, breathing (as YouTube Music's own wait). */
export function Skeletons({ shelves = 3, rows = false }: { shelves?: number; rows?: boolean }) {
  return (
    <div className="ytm-skeletons" role="progressbar">
      {Array.from({ length: shelves }, (_, index) => (
        <section key={index} className="ytm-section">
          <span className="ytm-skeleton-line title" />
          {rows ? (
            <div className="ytm-songs">
              {Array.from({ length: 5 }, (_, at) => (
                <div key={at} className="ytm-row">
                  <span className="ytm-row-cover">
                    <span className="ytm-cover ytm-skeleton-block" />
                  </span>
                  <span className="ytm-row-text">
                    <span className="ytm-skeleton-line" />
                    <span className="ytm-skeleton-line short" />
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <div className="ytm-shelf">
              {Array.from({ length: 6 }, (_, at) => (
                <div key={at} className="ytm-card">
                  <span className="ytm-card-cover">
                    <span className="ytm-cover ytm-skeleton-block" />
                  </span>
                  <span className="ytm-skeleton-line" />
                  <span className="ytm-skeleton-line short" />
                </div>
              ))}
            </div>
          )}
        </section>
      ))}
    </div>
  )
}

/** Couldn't load: said, with a way to ask again. */
export function Failed({ onRetry }: { onRetry(): void }) {
  const text = useText()
  return (
    <div className="ytm-failed">
      <p>{text.youtubeMusic.failed}</p>
      <button type="button" className="ytm-pill" onClick={onRetry}>
        {text.youtubeMusic.tryAgain}
      </button>
    </div>
  )
}
