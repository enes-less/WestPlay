import { type CSSProperties, type ReactNode, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { TextField } from '../../keyboard/TextField'
import { useLanguage, useText } from '../../state/text'
import {
  addToLibrary,
  type AppleMusicItem,
  type AppleMusicSection,
  type AppleMusicShelf,
  type AppleMusicTrack,
  clearRecent,
  type LibraryLook,
  openArtist,
  openItem,
  openView,
  playFrom,
  remember,
  setLibraryLook,
  signIn,
  useAccount,
  useAdded,
  useAppleMusic,
  useAsked,
  useShownAgain,
  type View,
} from './appleMusic'
import {
  AllGlyph,
  CheckGlyph,
  ChevronRightGlyph,
  FilterGlyph,
  GridGlyph,
  LibraryGlyph,
  ListGlyph,
  MoreGlyph,
  NoteGlyph,
  PlayGlyph,
  PlusGlyph,
  SearchGlyph,
  ShuffleGlyph,
  StarGlyph,
} from './glyphs'
import { useKindLine } from './look'
import { Cover, Explicit, Failed, Loading, Section, SongRow, Tile } from './parts'

/** The Music app's pages: each tab's own (Home, New, Radio, the Library, Search), and the pages over them. */

/** A page that scrolls, its large title on top (Apple's). */
function Scroll({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <div className="am-scroll">
      {title && <h1 className="am-title">{title}</h1>}
      {children}
    </div>
  )
}

/** A question's answer as a page: Apple's spinner while it comes, Try Again if it didn't. */
function Answered<T>({
  asked,
  title,
  children,
}: {
  asked: { answer: T | null; failed: boolean; retry(): void }
  title?: string
  children(answer: T): ReactNode
}) {
  if (asked.failed) {
    return (
      <Scroll title={title}>
        <Failed onRetry={asked.retry} />
      </Scroll>
    )
  }
  if (!asked.answer) {
    return (
      <Scroll title={title}>
        <Loading />
      </Scroll>
    )
  }
  return <>{children(asked.answer)}</>
}

/** Sections, the first of Home (what was played lately) named here. */
function Sections({ sections }: { sections: AppleMusicSection[] }) {
  const text = useText()
  return (
    <>
      {sections.map((section, i) => (
        <Section
          key={`${section.title}:${i}`}
          section={section}
          title={section.style === 'recent' ? text.appleMusic.recentlyPlayed : undefined}
        />
      ))}
    </>
  )
}

/** Apple's card asking to sign in (Home and the Library, signed out). */
function Pitch({ title, text: words }: { title: string; text: string }) {
  const text = useText()
  const signingIn = useAppleMusic((state) => state.signingIn)
  return (
    <div className="am-pitch">
      <span className="am-pitch-mark" aria-hidden>
        <NoteGlyph size={52} />
      </span>
      <div className="am-pitch-text">
        <h2>{title}</h2>
        <p>{signingIn ? text.appleMusic.finishOnPc : words}</p>
      </div>
      <button type="button" className="am-pill key" onClick={signIn} disabled={signingIn}>
        {text.appleMusic.signIn}
      </button>
    </div>
  )
}

/** Home: asked again each time it's shown again (what was played lately changes as it's used). */
export function Home({ shown }: { shown: boolean }) {
  const text = useText()
  const account = useAccount()
  const asked = useAsked(account === 'in' ? { ask: 'home' } : null, useShownAgain(shown))
  const latest = useAsked(account === 'out' ? { ask: 'new' } : null)
  const title = text.appleMusic.tabs.home
  if (account === 'unknown') {
    return (
      <Scroll title={title}>
        <Loading />
      </Scroll>
    )
  }
  if (account === 'out') {
    // Signed out, Home is Apple's ask to sign in, then what's new.
    return (
      <Scroll title={title}>
        <Pitch title={text.appleMusic.homePitch.title} text={text.appleMusic.homePitch.text} />
        {latest.answer && <Sections sections={latest.answer.sections} />}
      </Scroll>
    )
  }
  return (
    <Answered asked={asked} title={title}>
      {(page) => (
        <Scroll title={title}>
          {page.sections.length === 0 && <p className="am-empty">{text.appleMusic.empty}</p>}
          <Sections sections={page.sections} />
        </Scroll>
      )}
    </Answered>
  )
}

export function New() {
  const text = useText()
  const asked = useAsked({ ask: 'new' })
  return (
    <Answered asked={asked} title={text.appleMusic.tabs.new}>
      {(page) => (
        <Scroll title={text.appleMusic.tabs.new}>
          <Sections sections={page.sections} />
        </Scroll>
      )}
    </Answered>
  )
}

export function Radio() {
  const text = useText()
  const asked = useAsked({ ask: 'radio' })
  return (
    <Answered asked={asked} title={text.appleMusic.tabs.radio}>
      {(page) => (
        <Scroll title={text.appleMusic.tabs.radio}>
          <Sections sections={dedupe(page.sections)} />
        </Scroll>
      )}
    </Answered>
  )
}

/** Apple's Radio repeats its first big cards in a second carousel: once is enough here. */
function dedupe(sections: AppleMusicSection[]): AppleMusicSection[] {
  const seen = new Set<string>()
  return sections.filter((section) => {
    const key = `${section.style}:${section.items[0]?.id ?? section.songs[0]?.id ?? section.title}`
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
}

const SHELVES: AppleMusicShelf[] = ['playlists', 'artists', 'albums', 'songs']

export function Library({ shown }: { shown: boolean }) {
  const text = useText()
  const account = useAccount()
  const asked = useAsked(account === 'in' ? { ask: 'library', shelf: 'recent' } : null, useShownAgain(shown))
  const title = text.appleMusic.tabs.library
  if (account !== 'in') {
    return (
      <Scroll title={title}>
        {account === 'out' ? (
          <Pitch title={text.appleMusic.libraryPitch.title} text={text.appleMusic.libraryPitch.text} />
        ) : (
          <Loading />
        )}
      </Scroll>
    )
  }
  return (
    <Scroll title={title}>
      <div className="am-list">
        {SHELVES.map((shelf) => (
          <button
            key={shelf}
            type="button"
            className="am-list-row"
            onClick={() => openView({ kind: 'library', shelf })}
          >
            <LibraryRowGlyph shelf={shelf} />
            <span>{shelfName(text, shelf)}</span>
            <ChevronRightGlyph size={28} />
          </button>
        ))}
      </div>
      <section className="am-section">
        <div className="am-section-head">
          <h2>{text.appleMusic.recentlyAdded}</h2>
        </div>
        {asked.failed ? (
          <Failed onRetry={asked.retry} />
        ) : !asked.answer ? (
          <Loading />
        ) : asked.answer.items.length === 0 ? (
          <p className="am-empty">{text.appleMusic.empty}</p>
        ) : (
          <div className="am-grid">
            {asked.answer.items.map((item, i) => (
              <Tile key={`${item.id}:${i}`} item={item} />
            ))}
          </div>
        )}
      </section>
    </Scroll>
  )
}

function shelfName(text: ReturnType<typeof useText>, shelf: AppleMusicShelf): string {
  const words = text.appleMusic
  return shelf === 'playlists'
    ? words.playlists
    : shelf === 'artists'
      ? words.artists
      : shelf === 'albums'
        ? words.albums
        : shelf === 'songs'
          ? words.songs
          : words.recentlyAdded
}

/** The Library's rows' glyphs, in Apple Music's key colour. */
function LibraryRowGlyph({ shelf }: { shelf: AppleMusicShelf }) {
  return (
    <span className="am-list-glyph" aria-hidden>
      {shelf === 'songs' ? <NoteGlyph size={30} /> : <LibraryGlyph size={30} />}
    </span>
  )
}

/** Search's own page: the field is in the app's bar; under it, what was searched lately and Apple Music's
 * categories. */
export function Search() {
  const text = useText()
  const recent = useAppleMusic((state) => state.recent)
  const asked = useAsked({ ask: 'browse' })
  return (
    <Scroll title={text.appleMusic.tabs.search}>
      {recent.length > 0 && (
        <section className="am-section">
          <div className="am-section-head">
            <h2>{text.appleMusic.recentlySearched}</h2>
            <button type="button" className="am-link" onClick={clearRecent}>
              {text.appleMusic.clear}
            </button>
          </div>
          <div className="am-shelf">
            {recent.map((item) => (
              <Tile key={item.id} item={item} />
            ))}
          </div>
        </section>
      )}
      <section className="am-section">
        <div className="am-section-head">
          <h2>{text.appleMusic.browseCategories}</h2>
        </div>
        {asked.failed ? (
          <Failed onRetry={asked.retry} />
        ) : !asked.answer ? (
          <Loading />
        ) : (
          <div className="am-categories">
            {asked.answer.map((category) => (
              <button
                key={category.id}
                type="button"
                className="am-category"
                style={{ '--cover': category.color } as CSSProperties}
                onClick={() => openItem(category)}
              >
                {category.image && <img src={category.image} alt="" draggable={false} />}
                <b>{category.name}</b>
              </button>
            ))}
          </div>
        )}
      </section>
    </Scroll>
  )
}

/** What's typed in the search field: the results as it's typed (a pause in the typing before asking). */
export function Typing({ query, onPicked }: { query: string; onPicked(): void }) {
  const [asked, setAsked] = useState(query.trim())
  useEffect(() => {
    const timer = setTimeout(() => setAsked(query.trim()), 250)
    return () => clearTimeout(timer)
  }, [query])
  if (!asked) return <Search />
  return <Results query={asked} onPicked={onPicked} />
}

function Results({ query, onPicked }: { query: string; onPicked?(): void }) {
  const text = useText()
  const asked = useAsked({ ask: 'search', query })
  const kindLine = useKindLine()
  const words = text.appleMusic
  return (
    <Answered asked={asked}>
      {(results) => {
        const none =
          results.top.length +
            results.songs.length +
            results.artists.length +
            results.albums.length +
            results.playlists.length +
            results.stations.length ===
          0
        if (none) {
          return (
            <Scroll>
              <div className="am-none">
                <h2>{words.noResults}</h2>
                <p>{words.noResultsHint}</p>
              </div>
            </Scroll>
          )
        }
        const pick = (item: AppleMusicItem) => {
          remember(item)
          onPicked?.()
          openItem(item)
        }
        const shelf = (title: string, items: AppleMusicItem[]) =>
          items.length > 0 && (
            <section className="am-section">
              <div className="am-section-head">
                <h2>{title}</h2>
              </div>
              <div className="am-shelf">
                {items.map((item) => (
                  <PickedTile key={item.id} item={item} onPick={pick} />
                ))}
              </div>
            </section>
          )
        return (
          <Scroll>
            {results.top.length > 0 && (
              <section className="am-section">
                <div className="am-section-head">
                  <h2>{words.topResults}</h2>
                </div>
                <div className="am-shelf am-top-results">
                  {results.top.map((item) => (
                    <button key={item.id} type="button" className="am-top-result" onClick={() => pick(item)}>
                      <Cover item={item} />
                      <span className="am-song-text">
                        <span className="am-song-name">
                          {item.name}
                          {item.explicit && <Explicit />}
                        </span>
                        <span className="am-song-by">{kindLine(item)}</span>
                      </span>
                    </button>
                  ))}
                </div>
              </section>
            )}
            {results.songs.length > 0 && (
              <Section section={{ title: words.songs, style: 'songs', items: [], songs: results.songs }} />
            )}
            {shelf(words.artists, results.artists)}
            {shelf(words.albums, results.albums)}
            {shelf(words.playlists, results.playlists)}
            {shelf(words.stations, results.stations)}
          </Scroll>
        )
      }}
    </Answered>
  )
}

/** A search's card: remembered as searched once picked. */
function PickedTile({ item, onPick }: { item: AppleMusicItem; onPick(item: AppleMusicItem): void }) {
  return (
    <button
      type="button"
      className={item.kind === 'artist' ? 'am-tile artist' : 'am-tile'}
      onClick={() => onPick(item)}
    >
      <Cover item={item} />
      <span className="am-tile-name">{item.name}</span>
      {item.kind !== 'artist' && item.by && <span className="am-tile-by">{item.by}</span>}
    </button>
  )
}

/** A page over a tab's own (`shown`: on top, showing). */
export function Page({ view, shown }: { view: View; shown: boolean }) {
  switch (view.kind) {
    case 'collection':
      return <CollectionPage item={view.item} />
    case 'artist':
      return <ArtistPage item={view.item} />
    case 'curator':
      return <CuratorPage item={view.item} />
    case 'results':
      return <Results query={view.query} />
    case 'library':
      return <LibraryShelfPage shelf={view.shelf} folder={view.folder} shown={shown} />
    case 'section':
      return <SectionPage section={view.section} />
  }
}

/** An album's or playlist's page: the cover beside its name, who it's by, Play and Shuffle; its songs under. */
function CollectionPage({ item }: { item: AppleMusicItem }) {
  const text = useText()
  const asked = useAsked({ ask: 'collection', id: item.id })
  const page = asked.answer
  const shown = page?.item ?? item
  const tracks = page?.tracks ?? []
  const album = shown.kind === 'album'
  const length = tracks.reduce((sum, track) => sum + track.duration, 0)
  const words = text.appleMusic
  return (
    <div className="am-scroll" style={{ '--cover': shown.color } as CSSProperties}>
      <header className="am-collection-head">
        <Cover item={shown} className="am-collection-cover" />
        <div className="am-collection-text">
          <h1>
            {shown.name}
            {shown.explicit && <Explicit />}
          </h1>
          {shown.by && (
            <p className="am-collection-by">
              {album && page?.tracks[0]?.artistId ? (
                <button type="button" onClick={() => openArtist(page.tracks[0].artistId!, shown.by)}>
                  {shown.by}
                </button>
              ) : (
                shown.by
              )}
            </p>
          )}
          {page && (page.genre || page.year) && (
            <p className="am-collection-meta">{[page.genre, page.year].filter(Boolean).join(' · ')}</p>
          )}
          <div className="am-actions">
            <button
              type="button"
              className="am-pill key"
              disabled={tracks.length === 0}
              onClick={() => playFrom(shown, tracks, null, false)}
            >
              <PlayGlyph size={26} />
              {words.play}
            </button>
            <button
              type="button"
              className="am-pill key"
              disabled={tracks.length === 0}
              onClick={() => playFrom(shown, tracks, null, true)}
            >
              <ShuffleGlyph size={26} />
              {words.shuffle}
            </button>
            <AddButton id={shown.id} />
          </div>
          {page?.description && <p className="am-collection-note">{plain(page.description)}</p>}
        </div>
      </header>
      {asked.failed && <Failed onRetry={asked.retry} />}
      {!page && !asked.failed && <Loading />}
      {page && (
        <>
          <div className="am-songs">
            {tracks.map((track, i) => (
              <SongRow
                key={`${track.id}:${i}`}
                track={track}
                contextId={shown.id}
                cover={!album}
                number={album ? (track.number ?? i + 1) : null}
                onPlay={() => playFrom(shown, tracks, i)}
                end={
                  !album ? null : track.artist !== shown.by ? (
                    <span className="am-song-aside">{track.artist}</span>
                  ) : null
                }
              />
            ))}
          </div>
          {tracks.length > 0 && <p className="am-foot">{words.length(tracks.length, length)}</p>}
        </>
      )}
    </div>
  )
}

/** Apple's editors' notes come with a little HTML: shown as plain text. */
function plain(html: string): string {
  return html
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<[^>]+>/g, '')
    .replace(/&amp;/g, '&')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, '’')
}

/** Add to Library (+), a check once added. */
function AddButton({ id }: { id: string }) {
  const text = useText()
  const added = useAdded(id)
  return (
    <button
      type="button"
      className="am-round"
      aria-label={text.appleMusic.addToLibrary}
      disabled={added}
      onClick={() => addToLibrary(id, { added: text.appleMusic.added })}
    >
      {added ? <CheckGlyph size={28} /> : <PlusGlyph size={28} />}
    </button>
  )
}

/** An artist's page: their picture across the top, their name on it and Play; their top songs, then their
 * albums and more. */
function ArtistPage({ item }: { item: AppleMusicItem }) {
  const text = useText()
  const asked = useAsked({ ask: 'artist', id: item.id })
  const page = asked.answer
  const shown = page?.item ?? item
  const picture = shown.wide ?? shown.image
  return (
    <div className="am-scroll am-artist" style={{ '--cover': shown.color } as CSSProperties}>
      <header className="am-artist-head">
        {picture && <img src={picture} alt="" draggable={false} />}
        <div className="am-artist-name">
          <button
            type="button"
            className="am-play-round"
            aria-label={text.appleMusic.play}
            disabled={!page || page.top.length === 0}
            onClick={() => page && playFrom(shown, page.top, null, false)}
          >
            <PlayGlyph size={34} />
          </button>
          <h1>{shown.name}</h1>
        </div>
      </header>
      {asked.failed && <Failed onRetry={asked.retry} />}
      {!page && !asked.failed && <Loading />}
      {page && (
        <>
          {page.top.length > 0 && (
            <Section section={{ title: text.appleMusic.topSongs, style: 'songs', items: [], songs: page.top }} />
          )}
          {page.sections.map((section, i) => (
            <Section key={`${section.title}:${i}`} section={section} />
          ))}
        </>
      )}
    </div>
  )
}

/** A category's page: its name, and Apple's editors' sections for it. */
function CuratorPage({ item }: { item: AppleMusicItem }) {
  const asked = useAsked({ ask: 'curator', id: item.id })
  return (
    <Answered asked={asked} title={item.name}>
      {(page) => (
        <Scroll title={item.name}>
          <Sections sections={page.sections} />
        </Scroll>
      )}
    </Answered>
  )
}

/** A section's See All: its cards in a grid, or its songs. */
function SectionPage({ section }: { section: AppleMusicSection }) {
  return (
    <Scroll title={section.title}>
      {section.songs.length > 0 ? (
        <div className="am-songs">
          {section.songs.map((track, i) => (
            <SongRow key={`${track.id}:${i}`} track={track} cover onPlay={() => playFrom(null, section.songs, i)} />
          ))}
        </div>
      ) : (
        <div className="am-grid">
          {section.items.map((item, i) => (
            <Tile key={`${item.id}:${i}`} item={item} />
          ))}
        </div>
      )}
    </Scroll>
  )
}

/** The filter's name for all of a list ("All Playlists"). */
function allOf(text: ReturnType<typeof useText>, shelf: AppleMusicShelf): string {
  const all = text.appleMusic.all
  return shelf === 'albums' ? all.albums : shelf === 'songs' ? all.songs : all.playlists
}

/** Which lists can show only favourites (Apple Music rates playlists, albums and songs). */
const FAVOURABLE: AppleMusicShelf[] = ['playlists', 'albums', 'songs']

/**
 * A list of the Library, as Apple's (iOS 26): its title, and on its right the filter (all, or favourites:
 * Apple Music's red while it filters) and More (a grid or a list, and the sorts); a search field to find in
 * it; then the list, a favourite's star beside it. The sorts are Apple Music's own (playlists': its web
 * player's three), as chosen; showing as a grid or list, filtering and finding are here.
 */
function LibraryShelfPage({
  shelf,
  folder,
  shown,
}: {
  shelf: AppleMusicShelf
  folder?: AppleMusicItem
  shown: boolean
}) {
  const text = useText()
  const language = useLanguage()
  const look = useAppleMusic((state) => state.libraryLooks[shelf])
  const asked = useAsked(
    {
      ask: 'library',
      shelf,
      ...(look.sort ? { sort: look.sort } : {}),
      ...(folder ? { folder: folder.id } : {}),
      language,
    },
    useShownAgain(shown),
  )
  // A new sort shows the list as it was until the sorted one comes.
  const [kept, setKept] = useState(asked.answer)
  if (asked.answer && asked.answer !== kept) setKept(asked.answer)
  const found = asked.answer ?? kept
  const [query, setQuery] = useState('')
  const title = folder?.name ?? shelfName(text, shelf)
  const wanted = query.trim().toLocaleLowerCase()
  const matches = (name: string, by: string) =>
    !wanted || name.toLocaleLowerCase().includes(wanted) || by.toLocaleLowerCase().includes(wanted)
  const favourites = look.filter === 'favorites'
  const items = (found?.items ?? []).filter((item) => (!favourites || item.favorite) && matches(item.name, item.by))
  const songs = (found?.songs ?? []).filter((song) => (!favourites || song.favorite) && matches(song.name, song.artist))
  return (
    <Scroll>
      <div className="am-page-head">
        <h1 className="am-title">{title}</h1>
        <LibraryControls shelf={shelf} look={look} sorts={found?.sorts ?? []} sort={found?.sort ?? null} />
      </div>
      <div className="am-find">
        <TextField
          id={`apple-music-library-${shelf}`}
          value={query}
          onChange={setQuery}
          hint={text.appleMusic.find}
          label={text.appleMusic.find}
          icon={<SearchGlyph size={28} />}
        />
      </div>
      {asked.failed && !found && <Failed onRetry={asked.retry} />}
      {!found && !asked.failed && <Loading />}
      {found && items.length + songs.length === 0 && <p className="am-empty">{text.appleMusic.empty}</p>}
      {songs.length > 0 && <SongList songs={songs} />}
      {items.length > 0 &&
        (look.view === 'grid' ? (
          <div className="am-grid">
            {items.map((item, i) => (
              <Tile key={`${item.id}:${i}`} item={item} />
            ))}
          </div>
        ) : (
          <div className="am-list">
            {items.map((item, i) => (
              <ListRow key={`${item.id}:${i}`} item={item} />
            ))}
          </div>
        ))}
    </Scroll>
  )
}

/** A row of a Library list: a favourite's star in the margin, the cover, the name (and who it's by), the
 * chevron. */
function ListRow({ item }: { item: AppleMusicItem }) {
  return (
    <button type="button" className="am-list-item" onClick={() => openItem(item)}>
      <span className="am-list-star" aria-hidden>
        {item.favorite && <StarGlyph size={18} on />}
      </span>
      <Cover item={item} />
      <span className="am-song-text">
        <span className="am-list-name">{item.name}</span>
        {item.kind === 'album' && item.by && <span className="am-song-by">{item.by}</span>}
      </span>
      <ChevronRightGlyph size={28} />
    </button>
  )
}

/** Where a menu sits in the app: under its buttons, their right edges together (CSS pixels of the app). */
type MenuPlace = { app: HTMLElement; top: number; right: number }

/** The filter and More, in a glass capsule; each opens its menu under it. The menu is drawn in the app, not
 *  in the page's scroller: glass inside a scroller leaves the text and hairlines under it sharp (Chromium
 *  composites them over the blur), and over it the veil keeps the page from scrolling away from the menu. */
function LibraryControls({
  shelf,
  look,
  sorts,
  sort,
}: {
  shelf: AppleMusicShelf
  look: LibraryLook
  sorts: { id: string; label: string }[]
  sort: string | null
}) {
  const text = useText()
  const [menu, setMenu] = useState<'filter' | 'more' | null>(null)
  const [place, setPlace] = useState<MenuPlace | null>(null)
  const controls = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const app = controls.current?.closest<HTMLElement>('.am-app')
    if (!menu || !app || !controls.current) return
    const outer = app.getBoundingClientRect()
    const inner = controls.current.getBoundingClientRect()
    // The cab may draw the app scaled: back to its own pixels.
    const scale = outer.width / app.offsetWidth || 1
    setPlace({ app, top: (inner.bottom - outer.top) / scale + 10, right: (outer.right - inner.right) / scale })
  }, [menu])
  const words = text.appleMusic
  const filtering = look.filter !== 'all'
  const choose = (change: Partial<LibraryLook>) => {
    setLibraryLook(shelf, change)
    setMenu(null)
  }
  const views = shelf !== 'songs'
  return (
    <div className="am-controls" ref={controls}>
      <div className="am-control-bar glass">
        {FAVOURABLE.includes(shelf) && (
          <button
            type="button"
            className={filtering ? 'am-control on' : 'am-control'}
            aria-label={words.filter}
            aria-expanded={menu === 'filter'}
            onClick={() => setMenu(menu === 'filter' ? null : 'filter')}
          >
            <FilterGlyph size={30} />
          </button>
        )}
        {(views || sorts.length > 0) && (
          <button
            type="button"
            className="am-control"
            aria-label={words.more}
            aria-expanded={menu === 'more'}
            onClick={() => setMenu(menu === 'more' ? null : 'more')}
          >
            <MoreGlyph size={30} />
          </button>
        )}
      </div>
      {menu &&
        place &&
        createPortal(
          <>
            <div className="am-menu-veil" onClick={() => setMenu(null)} />
            {menu === 'filter' ? (
              <div className="am-popover glass" role="menu" style={{ top: place.top, right: place.right }}>
                <MenuItem on={!filtering} glyph={<AllGlyph size={28} />} onPick={() => choose({ filter: 'all' })}>
                  {allOf(text, shelf)}
                </MenuItem>
                <hr />
                <MenuItem on={filtering} glyph={<StarGlyph size={28} />} onPick={() => choose({ filter: 'favorites' })}>
                  {words.favorites}
                </MenuItem>
              </div>
            ) : (
              <div className="am-popover glass" role="menu" style={{ top: place.top, right: place.right }}>
                {views && (
                  <>
                    <MenuItem
                      on={look.view === 'grid'}
                      glyph={<GridGlyph size={28} />}
                      onPick={() => choose({ view: 'grid' })}
                    >
                      {words.grid}
                    </MenuItem>
                    <MenuItem
                      on={look.view === 'list'}
                      glyph={<ListGlyph size={28} />}
                      onPick={() => choose({ view: 'list' })}
                    >
                      {words.list}
                    </MenuItem>
                  </>
                )}
                {views && sorts.length > 0 && <hr />}
                {sorts.map((each) => (
                  <MenuItem key={each.id} on={each.id === sort} onPick={() => choose({ sort: each.id })}>
                    {each.label || words.sorts[each.id as keyof typeof words.sorts] || each.id}
                  </MenuItem>
                ))}
              </div>
            )}
          </>,
          place.app,
        )}
    </div>
  )
}

/** A menu's choice: its check while it's chosen, its glyph, its words. */
function MenuItem({
  on,
  glyph,
  onPick,
  children,
}: {
  on: boolean
  glyph?: ReactNode
  onPick(): void
  children: ReactNode
}) {
  return (
    <button type="button" role="menuitemradio" aria-checked={on} className="am-menu-item" onClick={onPick}>
      <span className="am-menu-check">{on && <CheckGlyph size={24} />}</span>
      {glyph && <span className="am-menu-glyph">{glyph}</span>}
      <span>{children}</span>
    </button>
  )
}

/** The Library's songs: Play and Shuffle over them, as Apple's list has. */
function SongList({ songs }: { songs: AppleMusicTrack[] }) {
  const text = useText()
  return (
    <>
      <div className="am-actions">
        <button type="button" className="am-pill key" onClick={() => playFrom(null, songs, null, false)}>
          <PlayGlyph size={26} />
          {text.appleMusic.play}
        </button>
        <button type="button" className="am-pill key" onClick={() => playFrom(null, songs, null, true)}>
          <ShuffleGlyph size={26} />
          {text.appleMusic.shuffle}
        </button>
      </div>
      <div className="am-songs">
        {songs.map((track, i) => (
          <SongRow key={`${track.id}:${i}`} track={track} cover onPlay={() => playFrom(null, songs, i)} />
        ))}
      </div>
    </>
  )
}
