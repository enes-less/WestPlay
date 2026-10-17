import { type ReactNode, useEffect, useRef, useState } from 'react'
import { useKeyboard } from '../../keyboard/store'
import { TextField } from '../../keyboard/TextField'
import { useLanguage, useText } from '../../state/text'
import { BackGlyph, ExploreGlyph, HomeGlyph, LibraryGlyph, MarkGlyph, PersonGlyph, SearchGlyph } from './glyphs'
import { MiniPlayer, NowPlaying, SongMenu } from './NowPlaying'
import { Explore, Home, Library, Page, Search, Typing } from './pages'
import {
  type Tab,
  TABS,
  goBack,
  openTab,
  openView,
  remember,
  showToast,
  signIn,
  signOut,
  useAccount,
  useSongsEnd,
  useWebPlayer,
  useYouTubeMusic,
} from './youtubeMusic'
import './youtubeMusic.css'

const FIELD = 'youtube-music-search'
const ALL_TABS: readonly Tab[] = [...TABS, 'search']

/**
 * YouTube Music: WestPlay's own app in the look of YouTube Music's tablet app, sized for the cab. On top, its
 * mark (Back over a page), Search and the account; under it the page, each tab's kept where it was left; at the
 * foot the mini player over the tabs (Home, Explore, Library); pressed, Now Playing rises over all.
 */
export function YouTubeMusicApp() {
  const text = useText()
  const language = useLanguage()
  const tab = useYouTubeMusic((state) => state.tab)
  const views = useYouTubeMusic((state) => state.views)
  const account = useAccount()
  const signingIn = useYouTubeMusic((state) => state.signingIn)
  const nowPlaying = useYouTubeMusic((state) => state.nowPlaying)
  const backdrop = useYouTubeMusic((state) => state.backdrop)
  const [query, setQuery] = useState('')
  // The tab Search was opened from: closing Search goes back to it.
  const [before, setBefore] = useState<Tab>('home')
  const typing = useKeyboard((state) => state.field?.id === FIELD)
  useWebPlayer(language === 'tr' ? 'tr' : 'en')
  useSongsEnd()
  // A play YouTube Music didn't confirm: said.
  const unplayed = useYouTubeMusic((state) => state.unplayed)
  useEffect(() => {
    if (unplayed > 0) showToast(text.youtubeMusic.cantPlay)
  }, [unplayed, text])
  useEffect(() => {
    if (typing) useYouTubeMusic.setState({ tab: 'search' })
  }, [typing])

  // The keyboard put away by the player while words are typed (its own hide key): what they'd find stays, as
  // a page of results; put away by the app (a result picked, a search made, a tab), nothing more.
  const leaving = useRef(false)
  const wasTyping = useRef(typing)
  useEffect(() => {
    const put = wasTyping.current && !typing
    wasTyping.current = typing
    if (!put) return
    const deliberate = leaving.current
    leaving.current = false
    const wanted = query.trim()
    if (deliberate || !wanted) return
    const { views: now } = useYouTubeMusic.getState()
    const top = now.search[now.search.length - 1]
    if (top?.kind === 'results' && top.query === wanted) return
    openView({ kind: 'results', query: wanted })
    // Only as the keyboard goes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [typing])

  const stopTyping = () => {
    if (useKeyboard.getState().field?.id === FIELD) leaving.current = true
    useKeyboard.getState().detach(FIELD)
  }
  const search = (wanted: string) => {
    const trimmed = wanted.trim()
    if (!trimmed) return
    setQuery(trimmed)
    stopTyping()
    remember(trimmed)
    useYouTubeMusic.setState({ tab: 'search' })
    openView({ kind: 'results', query: trimmed })
  }
  const go = (to: Tab) => {
    stopTyping()
    if (to === 'search' && tab !== 'search') setBefore(tab)
    openTab(to)
  }
  const closeSearch = () => {
    stopTyping()
    setQuery('')
    openTab(before)
  }
  const searching = tab === 'search'
  const pages = views[tab]
  const words = text.youtubeMusic

  return (
    <section className="app-panel ytm-app" aria-label={text.apps.youtubeMusic}>
      {/* Home's picture (YouTube Music's, as its mood is), behind its top: the top bar and the moods over it. */}
      {backdrop && tab === 'home' && views.home.length === 0 && !typing && (
        <div className="ytm-backdrop" aria-hidden>
          <img key={backdrop} src={backdrop} alt="" draggable={false} />
        </div>
      )}
      <header className="ytm-top-bar">
        {searching ? (
          <button
            type="button"
            className="ytm-round"
            aria-label={words.back}
            onClick={pages.length && !typing ? goBack : closeSearch}
          >
            <BackGlyph size={32} />
          </button>
        ) : pages.length > 0 ? (
          <button type="button" className="ytm-round" aria-label={words.back} onClick={goBack}>
            <BackGlyph size={32} />
          </button>
        ) : (
          <span className="ytm-brand" aria-hidden>
            <MarkGlyph size={34} />
            <b>Music</b>
          </span>
        )}
        {searching ? (
          <div className="ytm-search">
            <TextField
              id={FIELD}
              value={query}
              onChange={setQuery}
              onSubmit={() => search(query)}
              returnKind="search"
              hint={words.searchHint}
              label={words.tabs.search}
              icon={<SearchGlyph size={28} />}
            />
          </div>
        ) : (
          <span className="ytm-top-gap" />
        )}
        {!searching && (
          <button type="button" className="ytm-round" aria-label={words.tabs.search} onClick={() => go('search')}>
            <SearchGlyph size={32} />
          </button>
        )}
        {account === 'out' && (
          <button type="button" className="ytm-pill outline" onClick={signIn} disabled={signingIn}>
            {words.signIn}
          </button>
        )}
        {account === 'in' && <Account />}
      </header>
      {/* Layers, each kept as it is under the one over it (a tab left, or a page gone back from, is found as it
          was, its scroll and all): each tab's own page, the pages over it, then what's typed. */}
      <div className="ytm-body">
        {ALL_TABS.map((each) => (
          <TabLayers key={each} tab={each} shown={each === tab && !typing} onSearch={search} />
        ))}
        {typing && (
          <Layer shown opened={false}>
            <Typing query={query} onSearch={search} onFill={setQuery} onPicked={stopTyping} />
          </Layer>
        )}
      </div>
      {signingIn && <p className="ytm-signing-in">{words.finishOnPc}</p>}
      {/* Not under Now Playing (it's there, larger). */}
      {!typing && !nowPlaying && <MiniPlayer />}
      {!typing && (
        <nav className="ytm-tabs">
          {TABS.map((each) => {
            const on = each === tab || (each === before && tab === 'search')
            return (
              <button
                key={each}
                type="button"
                className={each === tab ? 'ytm-tab on' : 'ytm-tab'}
                aria-pressed={each === tab}
                onClick={() => go(each)}
              >
                <TabGlyph tab={each} on={on && each === tab} />
                <span>{words.tabs[each]}</span>
              </button>
            )
          })}
        </nav>
      )}
      <Toast />
      <SignInPrompt />
      <NowPlaying />
      <SongMenu />
    </section>
  )
}

function TabGlyph({ tab, on }: { tab: Tab; on: boolean }) {
  if (tab === 'home') return <HomeGlyph size={30} filled={on} />
  if (tab === 'explore') return <ExploreGlyph size={30} filled={on} />
  return <LibraryGlyph size={30} filled={on} />
}

function TabLayers({ tab, shown, onSearch }: { tab: Tab; shown: boolean; onSearch(query: string): void }) {
  const views = useYouTubeMusic((state) => state.views[tab])
  return (
    <>
      <Layer shown={shown && views.length === 0} opened={false}>
        <TabPage tab={tab} shown={shown && views.length === 0} onSearch={onSearch} />
      </Layer>
      {views.map((view, index) => (
        <Layer key={view.id} shown={shown && index === views.length - 1} opened>
          <Page view={view} />
        </Layer>
      ))}
    </>
  )
}

function TabPage({ tab, shown, onSearch }: { tab: Tab; shown: boolean; onSearch(query: string): void }) {
  switch (tab) {
    case 'home':
      return <Home shown={shown} />
    case 'explore':
      return <Explore />
    case 'library':
      return <Library shown={shown} />
    case 'search':
      return <Search onSearch={onSearch} />
  }
}

/** A layer: a page just opened comes in from the side; one shown again (Back, a tab chosen again) only fades
 * up, where it was. */
function Layer({ shown, opened, children }: { shown: boolean; opened: boolean; children: ReactNode }) {
  const [fresh, setFresh] = useState(opened)
  const className = !shown ? 'ytm-layer away' : fresh ? 'ytm-layer fresh' : 'ytm-layer'
  return (
    <div className={className} onAnimationEnd={(event) => event.target === event.currentTarget && setFresh(false)}>
      {children}
    </div>
  )
}

/** The account: its picture (a person, without one), its menu holding Sign out. */
function Account() {
  const text = useText()
  const avatar = useYouTubeMusic((state) => state.avatar)
  const [menu, setMenu] = useState(false)
  return (
    <div className="ytm-account">
      <button
        type="button"
        className="ytm-me"
        aria-label={text.youtubeMusic.account}
        aria-expanded={menu}
        onClick={() => setMenu(!menu)}
      >
        {avatar ? <img src={avatar} alt="" draggable={false} /> : <PersonGlyph size={36} />}
      </button>
      {menu && (
        <>
          <div className="ytm-menu-veil" onClick={() => setMenu(false)} />
          <div className="ytm-menu" role="menu">
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setMenu(false)
                signOut()
              }}
            >
              {text.youtubeMusic.signOut}
            </button>
          </div>
        </>
      )}
    </div>
  )
}

/** "Saved to library": a moment over the mini player. */
function Toast() {
  const toast = useYouTubeMusic((state) => state.toast)
  return toast ? (
    <p key={toast.id} className="ytm-toast" role="status">
      {toast.text}
    </p>
  ) : null
}

/** YouTube Music's ask to sign in first (the Library, a like, saving), over the app. */
function SignInPrompt() {
  const text = useText()
  const prompt = useYouTubeMusic((state) => state.prompt)
  if (!prompt) return null
  const words = text.youtubeMusic
  const close = () => useYouTubeMusic.setState({ prompt: null })
  return (
    <div className="ytm-prompt-veil" onClick={close}>
      <div
        className="ytm-prompt"
        role="dialog"
        aria-label={words.promptTitle}
        onClick={(event) => event.stopPropagation()}
      >
        <MarkGlyph size={56} />
        <h2>{words.promptTitle}</h2>
        <p>{words.promptFor[prompt]}</p>
        <div className="ytm-prompt-buttons">
          <button type="button" className="ytm-pill" onClick={close}>
            {words.notNow}
          </button>
          <button type="button" className="ytm-pill white" onClick={signIn}>
            {words.signIn}
          </button>
        </div>
      </div>
    </div>
  )
}
