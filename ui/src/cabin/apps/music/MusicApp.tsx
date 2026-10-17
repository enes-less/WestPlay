import { type CSSProperties, type ReactNode, type RefObject, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { useKeyboard } from '../../keyboard/store'
import { TextField } from '../../keyboard/TextField'
import { useText } from '../../state/text'
import {
  goBack,
  openTab,
  openView,
  showToast,
  signIn,
  signOut,
  type Tab,
  TABS,
  useAccount,
  useAppleMusic,
  useSongsEnd,
  useWebPlayer,
} from './appleMusic'
import {
  BackGlyph,
  CloseGlyph,
  HomeGlyph,
  LibraryGlyph,
  NewGlyph,
  PersonGlyph,
  RadioGlyph,
  SearchGlyph,
} from './glyphs'
import { type BarSizes, barLayout, framed } from './barLayout'
import { MiniPlayer, NowPlaying } from './NowPlaying'
import { Home, Library, New, Page, Radio, Search, Typing } from './pages'
import './music.css'
import './appleMusic.css'

const FIELD = 'apple-music-search'

/**
 * Music: Apple Music, WestPlay's own app in the look of Apple's own on iPadOS 26, sized for the cab. On top,
 * the floating glass tab bar (Home, New, Radio, Library: each glyph over its word, as iOS 26's), Search its own glass circle beside it that opens
 * into the search field; each tab keeps its pages where they were left (Back, on the left, goes back through
 * them). Signed out, Sign In on the right. At the foot the glass mini player; pressed, Now Playing (CarPlay's
 * own) rises over all.
 */
export function MusicApp() {
  const text = useText()
  const tab = useAppleMusic((state) => state.tab)
  const views = useAppleMusic((state) => state.views)
  const account = useAccount()
  const signingIn = useAppleMusic((state) => state.signingIn)
  const [query, setQuery] = useState('')
  // The tab Search was opened from: closing Search goes back to it.
  const [before, setBefore] = useState<Tab>('home')
  const typing = useKeyboard((state) => state.field?.id === FIELD)
  useWebPlayer()
  useSongsEnd()
  // A play Apple Music didn't confirm: said.
  const unplayed = useAppleMusic((state) => state.unplayed)
  useEffect(() => {
    if (unplayed > 0) showToast(text.appleMusic.cantPlay)
  }, [unplayed, text])
  // Typing in the field is being in Search.
  useEffect(() => {
    if (typing) useAppleMusic.setState({ tab: 'search' })
  }, [typing])

  // The keyboard put away by the player while results show (its own hide key): those results stay, as a page
  // (as Search pressed); put away by the app (a result picked, Search pressed, a tab), nothing more.
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
    const { views: now } = useAppleMusic.getState()
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
    useAppleMusic.setState({ tab: 'search' })
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
  const barRef = useRef<HTMLDivElement>(null)
  const startRef = useRef<HTMLDivElement>(null)
  const endRef = useRef<HTMLDivElement>(null)
  const tabsRef = useRef<HTMLDivElement>(null)
  const bar = useBar({ bar: barRef, start: startRef, end: endRef, tabs: tabsRef }, searching)
  const pages = views[tab]
  const words = text.appleMusic
  // Signed out, Home and the Library are Apple's asks to sign in; the tabs stay as Apple's.
  const tabs = TABS.filter((each) => each !== 'search')

  return (
    <>
      <section className="app-panel am-app" aria-label={text.apps.music}>
        <div ref={barRef} className={bar.ready ? 'am-bar ready' : 'am-bar'}>
          <div ref={startRef} className="am-bar-side">
            {pages.length > 0 && !typing && (
              <button type="button" className="am-round glass" aria-label={words.back} onClick={goBack}>
                <BackGlyph size={34} />
              </button>
            )}
          </div>
          {/* Searching, the tabs fold into a close button and the field takes their room (iOS 26's). */}
          <nav className={searching ? 'am-tabs searching' : 'am-tabs'}>
            <div className="am-tab-bar glass" style={bar.tabs}>
              <div ref={tabsRef} className="am-tab-list" aria-hidden={searching}>
                {tabs.map((each) => (
                  <button
                    key={each}
                    type="button"
                    className={each === tab && !typing ? 'am-tab on' : 'am-tab'}
                    aria-pressed={each === tab}
                    tabIndex={searching ? -1 : undefined}
                    onClick={() => go(each)}
                  >
                    <TabGlyph tab={each} on={each === tab && !typing} />
                    <span>{words.tabs[each]}</span>
                  </button>
                ))}
              </div>
              <button
                type="button"
                className="am-close-search"
                aria-label={words.close}
                aria-hidden={!searching}
                tabIndex={searching ? undefined : -1}
                onClick={closeSearch}
              >
                <CloseGlyph size={30} />
              </button>
            </div>
            <div className={searching ? 'am-search glass open' : 'am-search glass'} style={bar.search}>
              {searching ? (
                <TextField
                  id={FIELD}
                  value={query}
                  onChange={setQuery}
                  onSubmit={() => search(query)}
                  returnKind="search"
                  hint={words.searchHint}
                  label={words.tabs.search}
                  icon={<SearchGlyph size={30} />}
                />
              ) : (
                <button
                  type="button"
                  className="am-search-button"
                  aria-label={words.tabs.search}
                  onClick={() => go('search')}
                >
                  <SearchGlyph size={32} />
                </button>
              )}
            </div>
          </nav>
          <div ref={endRef} className="am-bar-side end">
            {account === 'out' && (
              <button type="button" className="am-sign-in glass" onClick={signIn} disabled={signingIn}>
                {words.signIn}
              </button>
            )}
            {account === 'in' && !typing && <Account />}
          </div>
        </div>
        {/* Layers, each kept as it is under the one over it (a tab left, or a page gone back from, is found
            as it was, its scroll and all): each tab's own page, the pages over it, then what's typed. */}
        <div className="am-body">
          {TABS.map((each) => (
            <TabLayers key={each} tab={each} shown={each === tab && !typing} />
          ))}
          {typing && (
            <Layer shown={true} opened={false}>
              <Typing query={query} onPicked={stopTyping} />
            </Layer>
          )}
        </div>
        {signingIn && <p className="am-signing-in glass">{words.finishOnPc}</p>}
        {/* What floats at the foot steps aside for the keyboard. */}
        <MiniPlayer aside={typing} />
        {!typing && <Toast />}
        <SignInPrompt />
      </section>
      <NowPlaying />
    </>
  )
}

/**
 * The bar's tab bar and search field, placed by measure (barLayout.ts): the bar's width, its sides' things
 * and the tabs' own, measured as they change. Placed only once measured (`ready`); from then on, a change of
 * place is a smooth move (appleMusic.css).
 */
function useBar(parts: Record<'bar' | 'start' | 'end' | 'tabs', RefObject<HTMLDivElement | null>>, searching: boolean) {
  const [sizes, setSizes] = useState<BarSizes | null>(null)
  const [ready, setReady] = useState(false)
  useLayoutEffect(() => {
    const { bar, start, end, tabs } = parts
    if (!bar.current || !start.current || !end.current || !tabs.current) return
    const measure = () =>
      setSizes({
        bar: bar.current?.clientWidth ?? 0,
        start: start.current?.offsetWidth ?? 0,
        end: end.current?.offsetWidth ?? 0,
        tabs: tabs.current?.offsetWidth ?? 0,
      })
    measure()
    const observer = new ResizeObserver(measure)
    for (const part of [bar.current, start.current, end.current, tabs.current]) observer.observe(part)
    // Moves only once placed: never from nowhere to the first place.
    const timer = setTimeout(() => setReady(true), 80)
    return () => {
      observer.disconnect()
      clearTimeout(timer)
    }
    // The parts are the component's own refs, the same throughout.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  if (!sizes) return { ready, tabs: undefined, search: undefined }
  // The tab bar laid out whole, the field laid out open; each slid and cut to how it shows now.
  const closed = barLayout(sizes, false)
  const open = barLayout(sizes, true)
  const now = searching ? open : closed
  return {
    ready,
    tabs: style(framed(closed.tabs, now.tabs)),
    search: style(framed(open.search, now.search)),
  }
}

function style(frame: ReturnType<typeof framed>): CSSProperties {
  return {
    left: frame.left,
    width: frame.width,
    transform: `translateX(${frame.shift}px)`,
    clipPath: `inset(0 ${frame.cut}px 0 0 round calc(var(--am-bar) / 2))`,
  }
}

/** A tab's glyph (our own drawings, in Apple's manner): over its word, filled while chosen. */
function TabGlyph({ tab, on }: { tab: Tab; on: boolean }) {
  switch (tab) {
    case 'home':
      return <HomeGlyph size={30} on={on} />
    case 'new':
      return <NewGlyph size={30} on={on} />
    case 'radio':
      return <RadioGlyph size={30} />
    case 'library':
      return <LibraryGlyph size={30} />
    case 'search':
      return <SearchGlyph size={30} />
  }
}

/** A tab's page and the pages over it: the top one showing while the tab is. */
function TabLayers({ tab, shown }: { tab: Tab; shown: boolean }) {
  const views = useAppleMusic((state) => state.views[tab])
  return (
    <>
      <Layer shown={shown && views.length === 0} opened={false}>
        <TabPage tab={tab} shown={shown && views.length === 0} />
      </Layer>
      {views.map((view, index) => (
        <Layer key={view.id} shown={shown && index === views.length - 1} opened={true}>
          <Page view={view} shown={shown && index === views.length - 1} />
        </Layer>
      ))}
    </>
  )
}

function TabPage({ tab, shown }: { tab: Tab; shown: boolean }) {
  switch (tab) {
    case 'home':
      return <Home shown={shown} />
    case 'new':
      return <New />
    case 'radio':
      return <Radio />
    case 'library':
      return <Library shown={shown} />
    case 'search':
      return <Search />
  }
}

/** A layer: a page just opened comes in from the side (where it's going, forward); one shown again (Back, a
 * tab chosen again) only fades up, where it was. */
function Layer({ shown, opened, children }: { shown: boolean; opened: boolean; children: ReactNode }) {
  const [fresh, setFresh] = useState(opened)
  const className = !shown ? 'am-layer away' : fresh ? 'am-layer fresh' : 'am-layer'
  return (
    <div className={className} onAnimationEnd={(event) => event.target === event.currentTarget && setFresh(false)}>
      {children}
    </div>
  )
}

/** The account: its picture (a person, without one), its menu holding Sign Out (Apple's account button). */
function Account() {
  const text = useText()
  const avatar = useAppleMusic((state) => state.avatar)
  const [menu, setMenu] = useState(false)
  return (
    <div className="am-account">
      <button
        type="button"
        className="am-me glass"
        aria-label={text.appleMusic.account}
        aria-expanded={menu}
        onClick={() => setMenu(!menu)}
      >
        {avatar ? <img src={avatar} alt="" draggable={false} /> : <PersonGlyph size={34} />}
      </button>
      {menu && (
        <div className="am-menu" role="menu">
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setMenu(false)
              signOut()
            }}
          >
            {text.appleMusic.signOut}
          </button>
        </div>
      )}
    </div>
  )
}

/** "Added to Library": a moment over the mini player. */
function Toast() {
  const toast = useAppleMusic((state) => state.toast)
  return toast ? (
    <p key={toast.id} className="am-toast glass" role="status">
      {toast.text}
    </p>
  ) : null
}

/** Apple's ask to sign in first (to keep music in the Library, to favourite), over the app. */
function SignInPrompt() {
  const text = useText()
  const prompt = useAppleMusic((state) => state.prompt)
  if (!prompt) return null
  const words = text.appleMusic
  const close = () => useAppleMusic.setState({ prompt: null })
  return (
    <div className="am-prompt-veil" onClick={close}>
      <div
        className="am-prompt"
        role="dialog"
        aria-label={words.promptTitle}
        onClick={(event) => event.stopPropagation()}
      >
        <h2>{words.promptTitle}</h2>
        <p>{words.promptFor[prompt]}</p>
        <div className="am-prompt-buttons">
          <button type="button" className="am-pill" onClick={close}>
            {words.notNow}
          </button>
          <button type="button" className="am-pill key" onClick={signIn}>
            {words.signIn}
          </button>
        </div>
      </div>
    </div>
  )
}
