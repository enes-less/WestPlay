import { type CSSProperties, type ReactNode, type UIEvent, useEffect, useState } from 'react'
import { useText } from '../../state/text'
import {
  CloseGlyph,
  FillGlyph,
  HistoryGlyph,
  PlayGlyph,
  RadioGlyph,
  SaveGlyph,
  SearchGlyph,
  ShuffleGlyph,
} from './glyphs'
import { useMore } from '../more'
import { Cover, Failed, Section, Skeletons, SongRow } from './parts'
import {
  type View,
  type YouTubeMusicCollection,
  type YouTubeMusicItem,
  type YouTubeMusicShelf,
  ask,
  forget,
  openItem,
  openPage,
  playWatch,
  remember,
  save,
  signIn,
  trackOf,
  useAccount,
  useAsked,
  useSaved,
  useShownAgain,
  useYouTubeMusic,
} from './youtubeMusic'

/** A page's scroller, asking for more as it nears its end (`onEnd`), saying how far it's scrolled (`onScrolled`). */
function Scroll({
  children,
  onEnd,
  onScrolled,
  className = '',
}: {
  children: ReactNode
  onEnd?(): void
  onScrolled?(top: number, element: HTMLDivElement): void
  className?: string
}) {
  const nearEnd = (event: UIEvent<HTMLDivElement>) => {
    const element = event.currentTarget
    onScrolled?.(element.scrollTop, element)
    if (onEnd && element.scrollTop + element.clientHeight > element.scrollHeight - element.clientHeight * 0.8) onEnd()
  }
  return (
    <div className={`ytm-scroll ${className}`} onScroll={nearEnd}>
      {children}
    </div>
  )
}

/** A question's answer as a page: its shapes while it comes, Failed (and Try again) if it doesn't. */
function Answered<A>({
  asked,
  rows,
  children,
}: {
  asked: { answer: A | null; failed: boolean; retry(): void }
  rows?: boolean
  children(answer: A): ReactNode
}) {
  if (asked.answer) return <>{children(asked.answer)}</>
  if (asked.failed) return <Failed onRetry={asked.retry} />
  return <Skeletons rows={rows} />
}

// --- The tabs' own pages -------------------------------------------------------------------------------------

/** Home: its moods (a chip chosen: Home of that mood), then its shelves, more as it's scrolled. */
export function Home({ shown }: { shown: boolean }) {
  const chip = useYouTubeMusic((state) => state.chip)
  const asked = useAsked({ ask: 'home', ...(chip ? { chip } : {}) }, useShownAgain(shown))
  const page = asked.answer
  const {
    items: sections,
    load,
    asking,
  } = useMore(page, page?.more, (more) =>
    ask({ ask: 'home', more }).then((next) => ({ items: next.sections, more: next.more })),
  )
  // YouTube Music's picture for Home (as its mood is), behind its top (YouTubeMusicApp.tsx draws it).
  const background = page?.background ?? null
  useEffect(() => {
    if (page) useYouTubeMusic.setState({ backdrop: background })
  }, [page, background])
  // How far Home is scrolled: its picture goes up with it.
  const scrolled = (top: number, element: HTMLDivElement) =>
    element.closest<HTMLElement>('.ytm-app')?.style.setProperty('--home-scrolled', `${Math.round(top)}px`)
  return (
    <Scroll onEnd={load} onScrolled={scrolled}>
      {page && page.chips.length > 0 && (
        <div className="ytm-chips">
          {page.chips.map((each) => {
            const on = Boolean(each.params && each.params === chip)
            return (
              <button
                key={each.label}
                type="button"
                className={on ? 'ytm-chip on' : 'ytm-chip'}
                onClick={() => useYouTubeMusic.setState({ chip: on ? null : (each.params ?? null) })}
              >
                {each.label}
              </button>
            )
          })}
        </div>
      )}
      <Answered asked={asked}>
        {(answer) => (
          <>
            {[...answer.sections, ...sections].map((section, index) => (
              <Section key={`${section.title}-${index}`} section={section} />
            ))}
            {asking && <Skeletons shelves={1} />}
          </>
        )}
      </Answered>
    </Scroll>
  )
}

export function Explore() {
  const asked = useAsked({ ask: 'explore' })
  return (
    <Scroll>
      <Answered asked={asked}>
        {(page) =>
          page.sections.map((section, index) => <Section key={`${section.title}-${index}`} section={section} />)
        }
      </Answered>
    </Scroll>
  )
}

const SHELVES: YouTubeMusicShelf[] = ['playlists', 'songs', 'albums', 'artists', 'subscriptions', 'history']

/** The Library: signed in, its lists (chips to choose); signed out, YouTube Music's ask to sign in. */
export function Library({ shown }: { shown: boolean }) {
  const text = useText()
  const account = useAccount()
  const signingIn = useYouTubeMusic((state) => state.signingIn)
  const [shelf, setShelf] = useState<YouTubeMusicShelf>('playlists')
  const asked = useAsked(account === 'in' ? { ask: 'library', shelf } : null, useShownAgain(shown))
  const words = text.youtubeMusic
  if (account !== 'in') {
    return (
      <Scroll className="ytm-pitch-page">
        {account === 'out' && (
          <div className="ytm-pitch">
            <h2>{words.libraryPitch.title}</h2>
            <p>{words.libraryPitch.text}</p>
            <button type="button" className="ytm-pill white" onClick={signIn} disabled={signingIn}>
              {words.signIn}
            </button>
          </div>
        )}
      </Scroll>
    )
  }
  return (
    <Scroll className="ytm-library">
      <div className="ytm-chips">
        {SHELVES.map((each) => (
          <button
            key={each}
            type="button"
            className={each === shelf ? 'ytm-chip on' : 'ytm-chip'}
            onClick={() => setShelf(each)}
          >
            {words.shelves[each]}
          </button>
        ))}
      </div>
      <Answered asked={asked} rows={shelf === 'songs' || shelf === 'history'}>
        {(page) =>
          page.sections.some((section) => section.items.length > 0) ? (
            page.sections.map((section, index) => <Section key={`${section.title}-${index}`} section={section} />)
          ) : (
            <p className="ytm-note">{words.empty}</p>
          )
        }
      </Answered>
    </Scroll>
  )
}

/** Search, before anything's typed: the searches made, newest first. */
export function Search({ onSearch }: { onSearch(query: string): void }) {
  const text = useText()
  const searches = useYouTubeMusic((state) => state.searches)
  return (
    <Scroll>
      <div className="ytm-history">
        {searches.map((query) => (
          <div key={query} className="ytm-history-row">
            <button type="button" className="ytm-history-words" onClick={() => onSearch(query)}>
              <HistoryGlyph size={26} />
              <span>{query}</span>
            </button>
            <button
              type="button"
              className="ytm-round small"
              aria-label={text.youtubeMusic.remove}
              onClick={() => forget(query)}
            >
              <CloseGlyph size={22} />
            </button>
          </div>
        ))}
      </div>
    </Scroll>
  )
}

/** What's typed so far: YouTube Music's words for it, and what it names straight away. */
export function Typing({
  query,
  onSearch,
  onFill,
  onPicked,
}: {
  query: string
  onSearch(query: string): void
  onFill(query: string): void
  onPicked(): void
}) {
  // Asked once the typing pauses a moment.
  const [settled, setSettled] = useState(query)
  useEffect(() => {
    const timer = setTimeout(() => setSettled(query), 220)
    return () => clearTimeout(timer)
  }, [query])
  const asked = useAsked(settled.trim() ? { ask: 'suggest', query: settled } : null)
  const found = asked.answer
  return (
    <Scroll>
      <div className="ytm-history">
        {(found?.texts ?? []).map((words) => (
          <div key={words} className="ytm-history-row">
            <button type="button" className="ytm-history-words" onClick={() => onSearch(words)}>
              <SearchGlyph size={26} />
              <span>{words}</span>
            </button>
            <button type="button" className="ytm-round small" aria-label={words} onClick={() => onFill(words)}>
              <FillGlyph size={22} />
            </button>
          </div>
        ))}
      </div>
      {(found?.items.length ?? 0) > 0 && (
        <div className="ytm-songs" onClickCapture={onPicked}>
          {found?.items.map((item, index) => (
            <SongRow key={`${item.title}-${index}`} item={item} />
          ))}
        </div>
      )}
    </Scroll>
  )
}

/** A search's results: its kinds to choose from, its best match with its songs, then all it found. */
function Results({ query }: { query: string }) {
  const text = useText()
  const [filter, setFilter] = useState<string | null>(null)
  const asked = useAsked({ ask: 'search', query, ...(filter ? { filter } : {}) })
  const found = asked.answer
  useEffect(() => remember(query), [query])
  const more = useMore(found, found?.more, (token) =>
    ask({ ask: 'search', query, ...(filter ? { filter } : {}), more: token }).then((next) => ({
      items: next.items,
      more: next.more,
    })),
  )
  const load = more.load
  const words = text.youtubeMusic
  return (
    <Scroll onEnd={load}>
      {found && found.chips.length > 0 && (
        <div className="ytm-chips">
          {found.chips.map((each) => {
            const on = Boolean(each.params && each.params === filter)
            return (
              <button
                key={each.label}
                type="button"
                className={on ? 'ytm-chip on' : 'ytm-chip'}
                onClick={() => setFilter(on ? null : (each.params ?? null))}
              >
                {each.label}
              </button>
            )
          })}
        </div>
      )}
      <Answered asked={asked} rows>
        {(answer) =>
          answer.items.length === 0 && !answer.top ? (
            <p className="ytm-note">{words.noResults}</p>
          ) : (
            <>
              {answer.top && <TopResult item={answer.top} songs={answer.topSongs} />}
              <div className="ytm-songs">
                {[...answer.items, ...more.items].map((item, index) => (
                  <SongRow key={`${item.browse?.id ?? item.video ?? item.title}-${index}`} item={item} />
                ))}
              </div>
              {more.asking && <Skeletons shelves={1} rows />}
            </>
          )
        }
      </Answered>
    </Scroll>
  )
}

/** A search's best match: its big card (an artist's round picture), playing it, and its songs beside. */
function TopResult({ item, songs }: { item: YouTubeMusicItem; songs: YouTubeMusicItem[] }) {
  const text = useText()
  return (
    <section className="ytm-top">
      <h2>{text.youtubeMusic.topResult}</h2>
      <div className="ytm-top-body">
        <div role="button" tabIndex={0} className="ytm-top-card" onClick={() => openItem(item)}>
          <Cover item={item} className="ytm-top-cover" />
          <span className="ytm-top-text">
            <span className="ytm-top-title">{item.title}</span>
            <span className="ytm-top-sub">{item.subtitle}</span>
            {item.play && (
              <button
                type="button"
                className="ytm-pill white"
                onClick={(event) => {
                  event.stopPropagation()
                  if (item.play) playWatch(item.play, item.video ? trackOf(item) : null)
                }}
              >
                <PlayGlyph size={24} />
                {text.youtubeMusic.play}
              </button>
            )}
          </span>
        </div>
        {songs.length > 0 && (
          <div className="ytm-songs">
            {songs.map((song, index) => (
              <SongRow key={`${song.video ?? song.title}-${index}`} item={song} />
            ))}
          </div>
        )}
      </div>
    </section>
  )
}

// --- The pages over the tabs' own --------------------------------------------------------------------------

/** What a list shows as it starts playing, until YouTube Music says its song: the list itself. */
function listShown(collection: YouTubeMusicCollection) {
  return {
    video: '',
    title: collection.title,
    artists: collection.by,
    album: '',
    ...(collection.image ? { image: collection.image } : {}),
    duration: 0,
    kind: 'song' as const,
    like: 'none' as const,
    hasVideo: false,
  }
}

/** An album's or playlist's page: its cover and what it is beside its songs (more as it's scrolled). */
function CollectionPage({ item }: { item: YouTubeMusicItem }) {
  const text = useText()
  const asked = useAsked(item.browse ? { ask: 'collection', id: item.browse.id } : null)
  const found = asked.answer
  const more = useMore(found, found?.more, (token) =>
    ask({ ask: 'collection', id: item.browse?.id ?? '', more: token }).then((next) => ({
      items: next.tracks,
      more: next.more,
    })),
  )
  const load = more.load
  const saved = useSaved(found?.playlist, found?.saved)
  const words = text.youtubeMusic
  const album = (found?.kind ?? item.kind) === 'album'
  return (
    <Scroll onEnd={load} className="ytm-collection">
      <aside className="ytm-collection-side">
        <Cover
          item={{ title: found?.title ?? item.title, kind: item.kind, image: found?.image ?? item.image }}
          className="ytm-collection-cover"
        />
        <h1>{found?.title ?? item.title}</h1>
        {found && (
          <>
            {found.by &&
              (found.byPage ? (
                <button
                  type="button"
                  className="ytm-link"
                  onClick={() => found.byPage && openPage('artist', found.byPage, found.by)}
                >
                  {found.by}
                </button>
              ) : (
                <span className="ytm-collection-by">{found.by}</span>
              ))}
            <span className="ytm-collection-line">{found.subtitle}</span>
            <span className="ytm-collection-line">{found.second}</span>
            {found.description && <p className="ytm-collection-words">{found.description}</p>}
            <div className="ytm-actions">
              {found.play && (
                <button
                  type="button"
                  className="ytm-pill white"
                  onClick={() =>
                    found.play && playWatch(found.play, found.tracks[0] ? trackOf(found.tracks[0]) : listShown(found))
                  }
                >
                  <PlayGlyph size={24} />
                  {words.play}
                </button>
              )}
              {found.shuffle && (
                <button
                  type="button"
                  className="ytm-round"
                  aria-label={words.shuffle}
                  onClick={() => found.shuffle && playWatch(found.shuffle, listShown(found))}
                >
                  <ShuffleGlyph size={26} />
                </button>
              )}
              {found.playlist && found.saved !== undefined && (
                <button
                  type="button"
                  className={saved ? 'ytm-round on' : 'ytm-round'}
                  aria-label={saved ? words.inLibrary : words.save}
                  aria-pressed={saved}
                  onClick={() =>
                    found.playlist && save(found.playlist, !saved, { saved: words.saved, removed: words.removed })
                  }
                >
                  <SaveGlyph size={26} saved={saved} />
                </button>
              )}
              {found.radio && (
                <button
                  type="button"
                  className="ytm-round"
                  aria-label={words.radio}
                  onClick={() => found.radio && playWatch(found.radio, listShown(found))}
                >
                  <RadioGlyph size={26} />
                </button>
              )}
            </div>
          </>
        )}
      </aside>
      <div className="ytm-collection-songs">
        <Answered asked={asked} rows>
          {(answer) => (
            <>
              <div className="ytm-songs">
                {[...answer.tracks, ...more.items].map((track, index) => (
                  <SongRow
                    key={`${track.video ?? track.title}-${index}`}
                    item={track}
                    number={album ? track.rank || String(index + 1) : undefined}
                  />
                ))}
              </div>
              {more.asking && <Skeletons shelves={1} rows />}
              {answer.sections.map((section, index) => (
                <Section key={`${section.title}-${index}`} section={section} />
              ))}
            </>
          )}
        </Answered>
      </div>
    </Scroll>
  )
}

/** An artist's page: their picture wide under their name, Shuffle and their radio, then their shelves. */
function ArtistPage({ item }: { item: YouTubeMusicItem }) {
  const text = useText()
  const asked = useAsked(item.browse ? { ask: 'artist', id: item.browse.id } : null)
  const found = asked.answer
  const words = text.youtubeMusic
  const image = found?.image ?? item.image
  const shown = (title: string) => ({ ...listShown({ title, by: '', image } as YouTubeMusicCollection) })
  return (
    <Scroll className="ytm-artist">
      <header className={image ? 'ytm-artist-head' : 'ytm-artist-head plain'}>
        {image && <img src={image} alt="" draggable={false} />}
        <div className="ytm-artist-name">
          <h1>{found?.name ?? item.title}</h1>
          {found?.subscribers && <span>{words.subscribers(found.subscribers)}</span>}
          {found && (
            <div className="ytm-actions">
              {found.shuffle && (
                <button
                  type="button"
                  className="ytm-pill white"
                  onClick={() => found.shuffle && playWatch(found.shuffle, shown(found.name))}
                >
                  <ShuffleGlyph size={24} />
                  {words.shuffle}
                </button>
              )}
              {found.radio && (
                <button
                  type="button"
                  className="ytm-pill outline"
                  onClick={() => found.radio && playWatch(found.radio, shown(found.name))}
                >
                  <RadioGlyph size={24} />
                  {words.radio}
                </button>
              )}
            </div>
          )}
        </div>
      </header>
      <Answered asked={asked}>
        {(page) =>
          page.sections.map((section, index) => <Section key={`${section.title}-${index}`} section={section} />)
        }
      </Answered>
    </Scroll>
  )
}

/** A page of YouTube Music's (a mood's, new releases, charts, a shelf's More). */
function BrowsePage({ view }: { view: Extract<View, { kind: 'browse' }> }) {
  const asked = useAsked({ ask: 'browse', page: view.page })
  return (
    <Scroll>
      <h1 className="ytm-page-title" style={view.color ? ({ '--mood': view.color } as CSSProperties) : undefined}>
        {asked.answer?.title || view.title}
      </h1>
      <Answered asked={asked}>
        {(page) =>
          page.sections.map((section, index) => <Section key={`${section.title}-${index}`} section={section} />)
        }
      </Answered>
    </Scroll>
  )
}

export function Page({ view }: { view: View }) {
  switch (view.kind) {
    case 'collection':
      return <CollectionPage item={view.item} />
    case 'artist':
      return <ArtistPage item={view.item} />
    case 'browse':
      return <BrowsePage view={view} />
    case 'results':
      return <Results query={view.query} />
    case 'library':
      return null
  }
}
