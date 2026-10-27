import { type CSSProperties, type UIEvent, useEffect, useState } from 'react'
import { useLive } from '../../state/store'
import { languageOf } from '../../state/strings'
import { useText } from '../../state/text'
import {
  AddGlyph,
  CloseGlyph,
  Equalizer,
  GridGlyph,
  ListGlyph,
  PauseGlyph,
  PlayGlyph,
  ShuffleGlyph,
  SortGlyph,
  VerifiedGlyph,
} from './glyphs'
import { useMore } from '../more'
import { trackCover, useKindLine, usePlaysFrom } from './look'
import { readKept } from './kept'
import {
  BrowseSkeleton,
  Card,
  Chips,
  Cover,
  ItemRow,
  PageSkeleton,
  RowsSkeleton,
  Shelf,
  ShelfSkeleton,
  ShortcutsSkeleton,
  TrackRow,
} from './parts'
import {
  ask,
  createPlaylist,
  openItem,
  openView,
  playFrom,
  playOrPause,
  remember,
  forget,
  type SpotifyItem,
  type SpotifyResults,
  type SpotifyTrack,
  type Tab,
  toggleSaved,
  toggleShuffle,
  useAccount,
  useAnswer,
  useAsked,
  useSaved,
  useSpotify,
  type View,
} from './spotify'

/** Spotify didn't answer: its own "Something went wrong", and Try again. */
function Failed({ onRetry }: { onRetry(): void }) {
  const text = useText()
  return (
    <div className="sp-failed">
      <h2>{text.spotify.failed}</h2>
      <button type="button" className="sp-pill white small" onClick={onRetry}>
        {text.spotify.tryAgain}
      </button>
    </div>
  )
}

/** A page Spotify didn't answer for. */
function FailedPage({ onRetry }: { onRetry(): void }) {
  return (
    <div className="sp-scroll sp-nothing">
      <Failed onRetry={onRetry} />
    </div>
  )
}

/** A page over a tab's own. */
export function Page({ view }: { view: View }) {
  switch (view.kind) {
    case 'collection':
      return <CollectionPage uri={view.uri} />
    case 'artist':
      return <ArtistPage uri={view.uri} />
    case 'results':
      return <Results query={view.query} />
    case 'shelf':
      return <ShelfPage title={view.title} items={view.items} />
  }
}

/** The colour of what plays (else `fallback`): Home's top is washed in it, as Spotify's is. */
function usePlayingColor(fallback: string | undefined): string | undefined {
  return useSpotify((state) => {
    const playback = state.playback
    return playback ? playback.tracks[playback.index]?.color : fallback
  })
}

// --- Home --------------------------------------------------------------------------------------------------

/** Home: the filters, the eight shortcuts, then the shelves Spotify picks. */
export function Home() {
  const text = useText()
  const account = useAccount()
  const [facet, setFacet] = useState('')
  // Signed out, Spotify's Home for anyone; not known yet, nothing (rather than the wrong one a moment).
  const signedIn = account === 'in'
  // Asked again as Home opens and a moment after another source starts playing (its Recently Played).
  const played = useSpotify((state) => state.played)
  const asked = useAsked(account === 'unknown' ? null : { ask: 'home', facet: signedIn ? facet : '', signedIn }, played)
  const home = asked.answer
  // Spotify's chips, from all of Home (a facet's answer may not repeat them): All, then Spotify's own.
  const all = useAnswer(account === 'unknown' ? null : { ask: 'home', facet: '', signedIn })
  const chips = signedIn ? (all?.chips ?? []) : []
  // What was kept of Home's Recently Played (kept.ts), shown while Spotify answers (all of Home only; not
  // signed out).
  const [kept] = useState(readKept)
  const showKept = !home && !facet && account !== 'out'
  return (
    <div className="sp-scroll sp-home">
      {chips.length > 0 && (
        <Chips
          options={[{ id: '', label: text.spotify.filters.all }, ...chips]}
          chosen={facet}
          onChoose={(id) => setFacet(facet === id ? '' : id)}
        />
      )}
      {asked.failed && <Failed onRetry={asked.retry} />}
      {!home && !asked.failed && (
        <>
          {showKept && kept.shortcuts.length > 0 ? (
            <div className="sp-shortcuts">
              {kept.shortcuts.map((item) => (
                <Shortcut key={item.uri} item={item} />
              ))}
            </div>
          ) : (
            <ShortcutsSkeleton />
          )}
          {showKept && kept.recent && kept.recent.items.length > 0 && (
            <Shelf title={kept.recent.title} items={kept.recent.items} onOpen={openItem} />
          )}
          <ShelfSkeleton />
          <ShelfSkeleton />
        </>
      )}
      {home && (
        <>
          {home.shortcuts.length > 0 && (
            <div className="sp-shortcuts">
              {home.shortcuts.slice(0, 8).map((item) => (
                <Shortcut key={item.uri} item={item} />
              ))}
            </div>
          )}
          {/* Spotify gives several shelves one title ("Made for you"): each keyed by its place too. */}
          {home.sections.map((section, index) => (
            <Shelf
              key={`${index}:${section.title}`}
              title={section.title}
              items={section.items}
              onOpen={openItem}
              action={
                section.items.length > 5 && (
                  <button
                    type="button"
                    className="sp-show-all"
                    onClick={() => openView({ kind: 'shelf', title: section.title, items: section.items })}
                  >
                    {text.spotify.showAll}
                  </button>
                )
              }
            />
          ))}
        </>
      )}
    </div>
  )
}

/**
 * The top of the app, behind the top bar too, as Spotify's: on Home, washed in the colour of what plays (else
 * of Home's first mix), fading into the black; on a playlist's, album's or artist's page, its cover's colour,
 * meeting the page's own (so the bar sits in the page's colour, as the page scrolls under it). Elsewhere, and
 * while typing, none.
 */
export function Wash({ tab, typing }: { tab: Tab; typing: boolean }) {
  const account = useAccount()
  const top = useSpotify((state) => state.views[tab].at(-1))
  const home = useAnswer(account === 'unknown' ? null : { ask: 'home', facet: '', signedIn: account === 'in' })
  const homeColor = usePlayingColor(home?.shortcuts[1]?.color ?? home?.sections[0]?.items[0]?.color)
  const collection = useAnswer(top?.kind === 'collection' ? { ask: 'collection', uri: top.uri } : null)
  const artist = useAnswer(top?.kind === 'artist' ? { ask: 'artist', uri: top.uri } : null)
  const pageColor = (top?.kind === 'collection' ? collection : top?.kind === 'artist' ? artist : null)?.item.color
  const mode = typing ? null : pageColor ? 'page' : tab === 'home' && !top ? 'home' : null
  const color = mode === 'page' ? pageColor : mode === 'home' ? homeColor : undefined
  return (
    <div
      className={mode ? `sp-wash ${mode}` : 'sp-wash away'}
      style={{ '--wash': color ?? 'transparent' } as CSSProperties}
    />
  )
}

/** One of Home's shortcuts: a cover and a name on a lighter strip; green, with the bars, while it plays. */
function Shortcut({ item }: { item: SpotifyItem }) {
  const { from, playing } = usePlaysFrom(item.uri)
  return (
    <button type="button" className="sp-shortcut" onClick={() => openItem(item)}>
      <Cover item={item} />
      <span className={from ? 'sp-shortcut-name sp-on' : 'sp-shortcut-name'}>{item.name}</span>
      {from && <EqualizerSlot playing={playing} />}
    </button>
  )
}

function EqualizerSlot({ playing }: { playing: boolean }) {
  return (
    <span className="sp-shortcut-eq">
      <Equalizer playing={playing} />
    </span>
  )
}

/** Show all: a shelf's cards, as a grid. */
function ShelfPage({ title, items }: { title: string; items: SpotifyItem[] }) {
  return (
    <div className="sp-scroll">
      <h2 className="sp-page-title">{title}</h2>
      <div className="sp-grid">
        {items.map((item) => (
          <Card key={item.uri} item={item} onOpen={openItem} />
        ))}
      </div>
    </div>
  )
}

// --- Search ------------------------------------------------------------------------------------------------

/** Search's own page: Browse all, tiles of colour with a cover leaning in the corner. */
export function Browse({ onSearch }: { onSearch(query: string): void }) {
  const text = useText()
  const asked = useAsked({ ask: 'browse' })
  const categories = asked.answer
  return (
    <div className="sp-scroll">
      <h2 className="sp-page-title">{text.spotify.browseAll}</h2>
      {asked.failed && <Failed onRetry={asked.retry} />}
      {!categories && !asked.failed && <BrowseSkeleton />}
      <div className="sp-browse">
        {categories?.map((category) => (
          <button
            key={category.name}
            type="button"
            className="sp-tile"
            style={{ background: category.color }}
            onClick={() => onSearch(category.name)}
          >
            <span className="sp-tile-name">{category.name.replaceAll('/', '/\u200b')}</span>
            {category.cover && <Cover item={category.cover} className="sp-tile-cover" />}
          </button>
        ))}
      </div>
    </div>
  )
}

/** As the player types: Spotify's results as they come; before anything's typed, the recent searches. */
export function Typing({ query, onPicked }: { query: string; onPicked(): void }) {
  const text = useText()
  const recent = useSpotify((state) => state.recent)
  // A pause in the typing before asking.
  const [asked, setAsked] = useState(query.trim())
  useEffect(() => {
    const timer = setTimeout(() => setAsked(query.trim()), 200)
    return () => clearTimeout(timer)
  }, [query])
  const results = useAnswer(asked ? { ask: 'search', query: asked } : null)
  const open = (item: SpotifyItem) => {
    remember(item)
    onPicked()
    openItem(item)
  }

  if (!query.trim()) {
    return (
      <div className="sp-scroll">
        {recent.length > 0 && <h2 className="sp-page-title small">{text.spotify.recentSearches}</h2>}
        <div className="sp-rows">
          {recent.map((item) => (
            <ItemRow
              key={item.uri}
              item={item}
              onOpen={open}
              end={
                <button
                  type="button"
                  className="sp-row-end"
                  aria-label={text.spotify.forget}
                  onClick={() => forget(item.uri)}
                >
                  <CloseGlyph />
                </button>
              }
            />
          ))}
        </div>
        {recent.length > 0 && (
          <button
            type="button"
            className="sp-pill outline small sp-clear-recent"
            onClick={() => useSpotify.setState({ recent: [] })}
          >
            {text.spotify.clearRecent}
          </button>
        )}
      </div>
    )
  }
  if (!results) return null
  // The top result first, as on the results' page; then songs, then the rest.
  const items = [
    ...results.artists.filter((a) => a.uri !== results.top?.uri).slice(0, 2),
    ...results.playlists.filter((p) => p.uri !== results.top?.uri).slice(0, 2),
    ...results.albums.slice(0, 2),
  ]
  return (
    <div className="sp-scroll">
      <div className="sp-rows">
        {results.top && <ItemRow item={results.top} onOpen={open} />}
        {results.tracks.slice(0, 3).map((track) => (
          <SongRow key={track.uri} track={track} others={results.tracks} onPlayed={onPicked} />
        ))}
        {items.map((item) => (
          <ItemRow key={item.uri} item={item} onOpen={open} />
        ))}
      </div>
    </div>
  )
}

/** A song found: pressed, it plays (its album's the place it plays from), the others found after it. */
function SongRow({ track, others, onPlayed }: { track: SpotifyTrack; others: SpotifyTrack[]; onPlayed?(): void }) {
  const text = useText()
  const album: SpotifyItem = {
    uri: track.album.uri,
    kind: 'album',
    name: track.album.name,
    by: track.artists[0]?.name ?? '',
    description: '',
    image: track.image,
    color: track.color,
  }
  const play = () => {
    const from = others.indexOf(track)
    playFrom(album, from >= 0 ? [...others.slice(from), ...others.slice(0, from)] : [track], 0)
    onPlayed?.()
  }
  const playing = useSpotify((state) => state.playback?.tracks[state.playback.index]?.uri === track.uri)
  return (
    <div className="sp-row">
      <button type="button" className="sp-row-open" onClick={play}>
        <Cover item={trackCover(track)} />
        <span className="sp-row-text">
          <span className={playing ? 'sp-row-name sp-on' : 'sp-row-name'}>{track.name}</span>
          <span className="sp-row-sub">
            {track.explicit && <span className="sp-explicit">{text.spotify.explicit}</span>}
            <span>
              {text.spotify.kinds.song} • {track.artists.map((a) => a.name).join(', ')}
            </span>
          </span>
        </span>
      </button>
    </div>
  )
}

type ResultsFilter = 'all' | 'songs' | 'artists' | 'playlists' | 'albums'

/** A search's results: the top result and its songs side by side, then a shelf of each; or one kind only. */
function Results({ query }: { query: string }) {
  const text = useText()
  const asked = useAsked({ ask: 'search', query })
  const results = asked.answer
  const [filter, setFilter] = useState<ResultsFilter>('all')
  if (asked.failed) return <FailedPage onRetry={asked.retry} />
  if (!results) {
    return (
      <div className="sp-scroll">
        <RowsSkeleton />
      </div>
    )
  }
  const empty =
    !results.top &&
    !results.topTrack &&
    results.tracks.length + results.artists.length + results.playlists.length + results.albums.length === 0
  if (empty) {
    return (
      <div className="sp-scroll sp-nothing">
        <h2>{text.spotify.noResults(query)}</h2>
        <p>{text.spotify.noResultsHint}</p>
      </div>
    )
  }
  const open = (item: SpotifyItem) => {
    remember(item)
    openItem(item)
  }
  return (
    <div className="sp-scroll">
      <Chips
        options={[
          { id: 'all', label: text.spotify.filters.all },
          { id: 'songs', label: text.spotify.songs },
          { id: 'artists', label: text.spotify.artists },
          { id: 'playlists', label: text.spotify.playlists },
          { id: 'albums', label: text.spotify.albums },
        ]}
        chosen={filter}
        onChoose={(id) => setFilter(filter === id ? 'all' : id)}
      />
      {filter === 'all' && (
        <>
          <div className="sp-top-results">
            <section className="sp-section">
              <div className="sp-section-head">
                <h3>{text.spotify.topResult}</h3>
              </div>
              <TopResult results={results} onOpen={open} />
            </section>
            {results.tracks.length > 0 && (
              <section className="sp-section sp-top-songs">
                <div className="sp-section-head">
                  <h3>{text.spotify.songs}</h3>
                </div>
                <div className="sp-rows">
                  {results.tracks.slice(0, 4).map((track) => (
                    <SongRow key={track.uri} track={track} others={results.tracks} />
                  ))}
                </div>
              </section>
            )}
          </div>
          <Shelf title={text.spotify.artists} items={results.artists} onOpen={open} />
          <Shelf title={text.spotify.playlists} items={results.playlists} onOpen={open} />
          <Shelf title={text.spotify.albums} items={results.albums} onOpen={open} />
        </>
      )}
      {filter === 'songs' && (
        <div className="sp-rows">
          {results.tracks.map((track) => (
            <SongRow key={track.uri} track={track} others={results.tracks} />
          ))}
        </div>
      )}
      {(filter === 'artists' || filter === 'playlists' || filter === 'albums') && (
        <div className="sp-grid">
          {results[filter].map((item) => (
            <Card key={item.uri} item={item} onOpen={open} />
          ))}
        </div>
      )}
    </div>
  )
}

/** The top result: a big cover (round for an artist), its name, what it is, and the green play. */
function TopResult({ results, onOpen }: { results: SpotifyResults; onOpen(item: SpotifyItem): void }) {
  const text = useText()
  const kindLine = useKindLine()
  const { top, topTrack } = results
  if (top) {
    return (
      <button type="button" className="sp-top-result" onClick={() => onOpen(top)}>
        <Cover item={top} />
        <span className="sp-top-name">{top.name}</span>
        <span className="sp-top-kind">{kindLine(top)}</span>
      </button>
    )
  }
  if (!topTrack) return null
  return (
    <button
      type="button"
      className="sp-top-result"
      onClick={() =>
        playFrom(
          {
            uri: topTrack.album.uri,
            kind: 'album',
            name: topTrack.album.name,
            by: '',
            description: '',
            image: topTrack.image,
            color: topTrack.color,
          },
          results.tracks,
          0,
        )
      }
    >
      <Cover item={trackCover(topTrack)} />
      <span className="sp-top-name">{topTrack.name}</span>
      <span className="sp-top-kind">
        {text.spotify.kinds.song} • {topTrack.artists.map((a) => a.name).join(', ')}
      </span>
    </button>
  )
}

// --- Your Library ------------------------------------------------------------------------------------------

type LibraryFilter = 'playlist' | 'artist' | 'album' | 'show'

export function Library() {
  const account = useAccount()
  // Not known yet (Spotify starting): the shapes of a list.
  if (account === 'unknown') {
    return (
      <div className="sp-scroll">
        <RowsSkeleton />
      </div>
    )
  }
  return account === 'in' ? <OwnLibrary /> : <FirstSteps />
}

/** Your Library signed out: Spotify's two first steps, a playlist to make (it asks to log in) and podcasts to
 * find (Browse's Podcasts). */
function FirstSteps() {
  const text = useText()
  const browsePodcasts = () => {
    useSpotify.setState({ tab: 'search' })
    openView({ kind: 'results', query: text.spotify.filters.podcasts })
  }
  return (
    <div className="sp-scroll">
      <h2 className="sp-page-title">{text.spotify.library}</h2>
      <div className="sp-first-steps">
        {(
          [
            [text.spotify.firstPlaylist, createPlaylist],
            [text.spotify.firstPodcasts, browsePodcasts],
          ] as const
        ).map(([words, action]) => (
          <section key={words.title} className="sp-first-step">
            <h3>{words.title}</h3>
            <p>{words.text}</p>
            <button type="button" className="sp-pill white small" onClick={action}>
              {words.button}
            </button>
          </section>
        ))}
      </div>
    </div>
  )
}

/** Your Library signed in: its filters (the one chosen green, a cross before it to clear it), Recents, and
 * the list or the grid. Liked Songs and what's pinned first. */
function OwnLibrary() {
  const text = useText()
  // Asked again as it opens (saved and unsaved elsewhere since).
  const asked = useAsked({ ask: 'library' }, 0)
  // What was kept of it (kept.ts), shown while Spotify answers.
  const [kept] = useState(readKept)
  const items = asked.answer ?? kept.library
  const [filter, setFilter] = useState<LibraryFilter | null>(null)
  const [grid, setGrid] = useState(false)
  const shown = (items ?? []).filter(
    (item) => !filter || item.kind === filter || (filter === 'playlist' && item.kind === 'liked'),
  )
  const options: { id: LibraryFilter; label: string }[] = [
    { id: 'playlist', label: text.spotify.playlists },
    { id: 'artist', label: text.spotify.artists },
    { id: 'album', label: text.spotify.albums },
    { id: 'show', label: text.spotify.podcasts },
  ]
  return (
    <div className="sp-scroll">
      <h2 className="sp-page-title">{text.spotify.library}</h2>
      {asked.failed && <Failed onRetry={asked.retry} />}
      <div className="sp-library-filters">
        {filter && (
          <button
            type="button"
            className="sp-chip sp-chip-clear"
            aria-label={text.spotify.filters.all}
            onClick={() => setFilter(null)}
          >
            <CloseGlyph />
          </button>
        )}
        <Chips
          options={filter ? options.filter((o) => o.id === filter) : options}
          chosen={filter}
          onChoose={(id) => setFilter(filter === id ? null : id)}
        />
      </div>
      <div className="sp-library-order">
        <span className="sp-sort">
          <SortGlyph />
          {text.spotify.recents}
        </span>
        <button
          type="button"
          className="sp-icon-button"
          aria-label={grid ? text.spotify.listView : text.spotify.gridView}
          onClick={() => setGrid(!grid)}
        >
          {grid ? <ListGlyph /> : <GridGlyph />}
        </button>
      </div>
      {!items && !asked.failed ? (
        <RowsSkeleton />
      ) : grid ? (
        <div className="sp-grid">
          {shown.map((item) => (
            <Card key={item.uri} item={item} onOpen={openItem} />
          ))}
        </div>
      ) : (
        <div className="sp-rows">
          {shown.map((item) => (
            <ItemRow key={item.uri} item={item} onOpen={openItem} />
          ))}
        </div>
      )}
    </div>
  )
}

// --- A playlist's or album's page --------------------------------------------------------------------------

/** A name's size: Spotify's titles are as large as the name lets them be. */
function titleSize(name: string): string {
  return name.length <= 12 ? 'huge' : name.length <= 22 ? 'large' : 'long'
}

/** The green Play (Pause, while this page's songs play). */
function PlayButton({ item, tracks }: { item: SpotifyItem; tracks: SpotifyTrack[] }) {
  const text = useText()
  const { playing } = usePlaysFrom(item.uri)
  return (
    <button
      type="button"
      className="sp-play"
      aria-label={playing ? text.pause : text.play}
      onClick={() => playOrPause(item, tracks)}
    >
      {playing ? <PauseGlyph /> : <PlayGlyph />}
    </button>
  )
}

function ShuffleButton() {
  const text = useText()
  const shuffle = useSpotify((state) => state.playback?.shuffle ?? false)
  return (
    <button
      type="button"
      className={shuffle ? 'sp-icon-button sp-toggle on' : 'sp-icon-button sp-toggle'}
      aria-label={text.spotify.shuffle}
      aria-pressed={shuffle}
      onClick={toggleShuffle}
    >
      <ShuffleGlyph />
    </button>
  )
}

function SaveButton({ item }: { item: SpotifyItem }) {
  const text = useText()
  const saved = useSaved(item.uri, item.saved)
  return (
    <button
      type="button"
      className={saved ? 'sp-icon-button sp-add added' : 'sp-icon-button sp-add'}
      aria-label={text.spotify.addToLibrary}
      aria-pressed={saved}
      onClick={() =>
        toggleSaved(item.uri, saved, { added: text.spotify.addedToLibrary, removed: text.spotify.removedFromLibrary })
      }
    >
      <AddGlyph added={saved} />
    </button>
  )
}

/** A playlist's, album's or Liked Songs' page: its songs, more of them as it's scrolled near its end (Spotify
 * answers 50 or 100 at a time). */
function CollectionPage({ uri }: { uri: string }) {
  const text = useText()
  const asked = useAsked({ ask: 'collection', uri })
  const page = asked.answer
  const more = useMore(page, page?.more, (from) =>
    ask({ ask: 'collection', uri, more: from }).then((next) => ({ items: next.tracks, more: next.more })),
  )
  if (asked.failed) return <FailedPage onRetry={asked.retry} />
  if (!page) return <PageSkeleton />
  const { item } = page
  const tracks = more.items.length > 0 ? [...page.tracks, ...more.items] : page.tracks
  const nearEnd = (event: UIEvent<HTMLDivElement>) => {
    const element = event.currentTarget
    if (element.scrollTop + element.clientHeight > element.scrollHeight - element.clientHeight * 0.8) more.load()
  }
  const length = tracks.reduce((sum, track) => sum + track.duration, 0)
  const album = item.kind === 'album'
  const meta = [
    page.owner,
    page.year,
    `${text.spotify.songCount(item.songs ?? tracks.length)}${item.songs ? '' : `, ${text.spotify.length(length)}`}`,
  ].filter(Boolean)
  return (
    <div className="sp-scroll sp-page" style={{ '--page': item.color } as CSSProperties} onScroll={nearEnd}>
      <header className="sp-header">
        <Cover item={item} className="sp-header-cover" />
        <div className="sp-header-text">
          <span className="sp-header-kind">{text.spotify.kinds[item.kind === 'track' ? 'song' : item.kind]}</span>
          <h2 className={`sp-header-name ${titleSize(item.name)}`}>{item.name}</h2>
          {page.description && <p className="sp-header-description">{page.description}</p>}
          <p className="sp-header-meta">
            {meta.map((part, i) => (
              <span key={i} className={i === 0 && page.owner ? 'sp-owner' : undefined}>
                {part}
              </span>
            ))}
          </p>
        </div>
      </header>
      <div className="sp-actions">
        <PlayButton item={item} tracks={tracks} />
        <ShuffleButton />
        {item.kind !== 'liked' && page.owner !== 'You' && <SaveButton item={item} />}
      </div>
      <div className="sp-tracks">
        {tracks.map((track, index) => (
          <TrackRow
            key={`${track.uri}:${index}`}
            track={track}
            number={album ? index + 1 : null}
            contextUri={item.uri}
            cover={!album}
            onPlay={() => playFrom(item, tracks, index)}
          />
        ))}
      </div>
    </div>
  )
}

// --- An artist's page --------------------------------------------------------------------------------------

function ArtistPage({ uri }: { uri: string }) {
  const text = useText()
  const language = languageOf(useLive((state) => state.gameLanguage))
  const asked = useAsked({ ask: 'artist', uri })
  const page = asked.answer
  const following = useSaved(uri, page?.item.saved)
  if (asked.failed) return <FailedPage onRetry={asked.retry} />
  if (!page) return <PageSkeleton round />
  const { item } = page
  return (
    <div className="sp-scroll sp-page" style={{ '--page': item.color } as CSSProperties}>
      <header className="sp-header sp-artist-header">
        <Cover item={item} className="sp-header-cover" />
        <div className="sp-header-text">
          {page.verified && (
            <span className="sp-verified">
              <VerifiedGlyph />
              {text.spotify.verified}
            </span>
          )}
          <h2 className={`sp-header-name ${titleSize(item.name)}`}>{item.name}</h2>
          <p className="sp-header-meta">
            <span>{text.spotify.monthlyListeners(new Intl.NumberFormat(language).format(page.listeners))}</span>
          </p>
        </div>
      </header>
      <div className="sp-actions">
        <PlayButton item={item} tracks={page.popular} />
        <ShuffleButton />
        <button
          type="button"
          className={following ? 'sp-pill outline small on' : 'sp-pill outline small'}
          aria-pressed={following}
          onClick={() =>
            toggleSaved(uri, following, {
              added: text.spotify.addedToLibrary,
              removed: text.spotify.removedFromLibrary,
            })
          }
        >
          {following ? text.spotify.following : text.spotify.follow}
        </button>
      </div>
      <section className="sp-section">
        <div className="sp-section-head">
          <h3>{text.spotify.popular}</h3>
        </div>
        <div className="sp-tracks">
          {page.popular.map((track, index) => (
            <TrackRow
              key={track.uri}
              track={track}
              number={index + 1}
              contextUri={item.uri}
              cover={true}
              onPlay={() => playFrom(item, page.popular, index)}
            />
          ))}
        </div>
      </section>
      <Shelf title={text.spotify.discography} items={page.releases} onOpen={openItem} />
    </div>
  )
}
