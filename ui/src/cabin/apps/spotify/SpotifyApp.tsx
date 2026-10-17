import { type ReactNode, useEffect, useState } from 'react'
import { useKeyboard } from '../../keyboard/store'
import { TextField } from '../../keyboard/TextField'
import { useText } from '../../state/text'
import { AccountButtons, LogInPrompt, PreviewBanner } from './account'
import { BackGlyph, BrowseGlyph, HomeGlyph, LibraryGlyph, SearchGlyph } from './glyphs'
import { MiniPlayer, NowPlaying } from './NowPlaying'
import { Browse, Home, Library, Page, Typing, Wash } from './pages'
import { goBack, openTab, openView, showToast, type Tab, useSongsEnd, useSpotify, useWebPlayer } from './spotify'
import '@fontsource-variable/figtree'
import './spotify.css'

const FIELD = 'spotify-search'

/**
 * The Spotify app, WestPlay's own in Spotify's look, sized for the cab: Spotify's near-black and green, its
 * type's nearest open cousin (Figtree, for Spotify's Circular), its chips, shortcuts, shelves and green Play.
 *
 * On top, as Spotify's own app on a wide screen: Home, the search pill ("What do you want to play?", Browse
 * at its end) and Your Library, each a tab that keeps its pages where they were left (Back goes back
 * through them), then the account (Sign up and Log in, signed out). At the foot, the mini player on the song's
 * colour (pressed, Now Playing rises over all); signed out, Spotify's banner for its preview.
 */
export function SpotifyApp() {
  const text = useText()
  const tab = useSpotify((state) => state.tab)
  const views = useSpotify((state) => state.views)
  const [query, setQuery] = useState('')
  const typing = useKeyboard((state) => state.field?.id === FIELD)
  useWebPlayer()
  useSongsEnd()
  // A play Spotify didn't confirm: said, as Spotify's own app says it.
  const unplayed = useSpotify((state) => state.unplayed)
  useEffect(() => {
    if (unplayed > 0) showToast(text.spotify.cantPlay)
  }, [unplayed, text])
  // Pressing the pill is going to Search.
  useEffect(() => {
    if (typing) useSpotify.setState({ tab: 'search' })
  }, [typing])

  const stopTyping = () => useKeyboard.getState().detach(FIELD)
  const search = (wanted: string) => {
    const trimmed = wanted.trim()
    if (!trimmed) return
    setQuery(trimmed)
    stopTyping()
    useSpotify.setState({ tab: 'search' })
    openView({ kind: 'results', query: trimmed })
  }
  const go = (to: Tab) => {
    stopTyping()
    openTab(to)
  }
  const pages = views[tab]

  return (
    <section className="app-panel spotify-app" aria-label={text.apps.spotify}>
      <Wash tab={tab} typing={typing} />
      <div className="sp-top">
        {pages.length > 0 && !typing ? (
          <button type="button" className="sp-round" aria-label={text.spotify.back} onClick={goBack}>
            <BackGlyph />
          </button>
        ) : (
          <button
            type="button"
            className={tab === 'home' && !typing ? 'sp-round on' : 'sp-round'}
            aria-label={text.spotify.home}
            aria-pressed={tab === 'home'}
            onClick={() => go('home')}
          >
            <HomeGlyph on={tab === 'home' && !typing} />
          </button>
        )}
        <div className={typing ? 'sp-search typing' : 'sp-search'}>
          <TextField
            id={FIELD}
            value={query}
            onChange={setQuery}
            onSubmit={() => search(query)}
            returnKind="search"
            hint={text.spotify.whatToPlay}
            label={text.spotify.whatToPlay}
            icon={<SearchGlyph />}
          />
          <span className="sp-search-rule" />
          <button
            type="button"
            className={tab === 'search' && pages.length === 0 && !typing ? 'sp-browse-button on' : 'sp-browse-button'}
            aria-label={text.spotify.browse}
            onClick={() => {
              setQuery('')
              go('search')
              if (tab === 'search') useSpotify.setState({ views: { ...views, search: [] } })
            }}
          >
            <BrowseGlyph />
          </button>
        </div>
        <button
          type="button"
          className={tab === 'library' && !typing ? 'sp-round on' : 'sp-round'}
          aria-label={text.spotify.library}
          aria-pressed={tab === 'library'}
          onClick={() => go('library')}
        >
          <LibraryGlyph on={tab === 'library' && !typing} />
        </button>
        {!typing && <AccountButtons />}
      </div>
      {/* Layers, each kept as it is under the one over it (a tab left, or a page gone back from, is found
          as it was, its scroll and all): each tab's own page, the pages over it, then what's typed. */}
      <div className="sp-body">
        {(['home', 'search', 'library'] as const).map((each) => (
          <TabLayers key={each} tab={each} shown={each === tab && !typing} onSearch={search} />
        ))}
        {typing && (
          <Layer shown={true} opened={false}>
            <Typing query={query} onPicked={stopTyping} />
          </Layer>
        )}
      </div>
      {/* What floats at the foot steps aside for the keyboard. */}
      <MiniPlayer aside={typing} />
      <PreviewBanner aside={typing} />
      {!typing && <Toast />}
      <NowPlaying />
      <LogInPrompt />
    </section>
  )
}

/** A tab's page and the pages over it: the top one showing while the tab is. */
function TabLayers({ tab, shown, onSearch }: { tab: Tab; shown: boolean; onSearch(query: string): void }) {
  const views = useSpotify((state) => state.views[tab])
  return (
    <>
      <Layer shown={shown && views.length === 0} opened={false}>
        {tab === 'home' ? <Home /> : tab === 'search' ? <Browse onSearch={onSearch} /> : <Library />}
      </Layer>
      {views.map((view, index) => (
        <Layer key={view.id} shown={shown && index === views.length - 1} opened={true}>
          <Page view={view} />
        </Layer>
      ))}
    </>
  )
}

/** A layer: a page just opened comes in from the side (where it's going, forward); one shown again (Back, a
 * tab chosen again) only fades up, where it was. */
function Layer({ shown, opened, children }: { shown: boolean; opened: boolean; children: ReactNode }) {
  const [fresh, setFresh] = useState(opened)
  const className = !shown ? 'sp-layer away' : fresh ? 'sp-layer fresh' : 'sp-layer'
  return (
    <div className={className} onAnimationEnd={(event) => event.target === event.currentTarget && setFresh(false)}>
      {children}
    </div>
  )
}

/** "Added to Liked Songs.": a moment over the mini player. */
function Toast() {
  const toast = useSpotify((state) => state.toast)
  return toast ? (
    <p key={toast.id} className="sp-toast" role="status">
      {toast.text}
    </p>
  ) : null
}
