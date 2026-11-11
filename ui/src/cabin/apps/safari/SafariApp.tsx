import { type CSSProperties, type ReactNode, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { SafariList, SafariTabState } from '../../../shared/host'
import { TextField } from '../../keyboard/TextField'
import { keyboardLanguage } from '../../keyboard/layouts'
import { type Typing, useKeyboard } from '../../keyboard/store'
import { useLanguage, useText } from '../../state/text'
import { engineName, shownAddress, siteOf } from './address'
import {
  BackGlyph,
  CheckGlyph,
  CloseGlyph,
  CompassGlyph,
  ForwardGlyph,
  GlobeGlyph,
  MoreGlyph,
  PageMenuGlyph,
  PlusGlyph,
  ReloadGlyph,
  SearchGlyph,
  SidebarGlyph,
  TabsGlyph,
  WarningGlyph,
} from './glyphs'
import { useCover } from './hole'
import {
  activeTab,
  answer,
  back,
  CLEAR_TIMEFRAMES,
  type ClearTimeframe,
  clearHistory,
  closeTab,
  forward,
  frequentlyVisited,
  goTo,
  historyMatches,
  load,
  newTab,
  pictureOf,
  reloadOrStop,
  selectTab,
  send,
  type Tab,
  tabName,
  timeframeStart,
  useSafari,
  type Visit,
  zoom,
} from './safari'
import { useLast, usePresence } from './presence'
import { pageColours } from './theme'
import { typedSteps } from './typed'
import { WebPage } from './WebPage'
import './safari.css'

/**
 * Safari (owner 2026-11-07), in iPad Safari's look (iPadOS 26, after Apple's iPad User Guide): the toolbar (the
 * sidebar button, Back, the address field with the page menu and Reload, New Tab and the tab overview in one
 * capsule) over the tab bar (Separate, iPad's default) or with the tabs in it (Compact, iPadOS 26.4's: Settings ›
 * Safari › Tabs); the page under them (the host's, shown through the cab: WebPage.tsx), the Start Page on a new
 * tab. The bars take the page's own colour (Show Color in Tab Bar). The sidebar's part the cab's Safari has is
 * History.
 */
export function SafariApp() {
  const text = useText()
  const tab = useSafari(activeTab)
  const tabs = useSafari((state) => state.tabs)
  const page = useSafari((state) => (state.active === null ? undefined : state.pages[state.active]))
  const view = useSafari((state) => state.view)
  const typing = useSafari((state) => state.typing)
  const compact = useSafari((state) => state.tabBar === 'compact')
  const tinted = useSafari((state) => state.pageColour)

  // Opened onto no tab: a new one, its address field typed into, as Safari opens.
  useEffect(() => {
    if (useSafari.getState().tabs.length === 0) newTab()
  }, [])

  // The tab overview and History come over the page and go again, animated (presence.ts); the page's own bars stay
  // under them as they go, and the page itself (the host's) shows again once they've gone, its picture till then.
  const overview = usePresence(view === 'tabs', LEAVE_MS)
  const history = usePresence(view === 'history', LEAVE_MS)
  const covered = overview !== null || history !== null
  const pageShown = view === 'page' || overview === 'out' || history === 'out'
  const showsPage = pageShown && !!tab?.url && !page?.failed
  const pageLive = showsPage && view === 'page' && !covered
  const full = showsPage && !!page?.fullScreen
  const colours = pageColours(tinted && showsPage && typing === null ? page?.theme : undefined)

  return (
    <section
      className={`app-panel safari-app${full ? ' full-screen' : ''}${colours ? ` page-${colours.look}` : ''}`}
      style={colours ? ({ '--sf-bar': colours.bar } as CSSProperties) : undefined}
      aria-label={text.apps.safari}
    >
      {pageShown && (
        <>
          {!full && <Toolbar tab={tab} page={page} compact={compact} />}
          {/* Not while the address is typed into: a cab's screen is short, and what's typed is suggested there. */}
          {!full && !compact && tabs.length > 1 && typing === null && <TabBar />}
          <div className="sf-content">
            {tab && !tab.url && <StartPage />}
            {tab && tab.url && page?.failed && <ErrorPage tab={tab} reason={page.failed} />}
            {tab && pageLive && <WebPage key={tab.id} tab={tab.id} url={tab.url} />}
            {tab && showsPage && !pageLive && <PagePicture tab={tab.id} />}
            {tab && pageLive && <WebField tab={tab.id} page={page} />}
            {tab && pageLive && <PageList tab={tab.id} list={page?.list} />}
          </div>
          <PageDialog />
        </>
      )}
      {overview && <TabOverview presence={overview} />}
      {history && <HistoryView presence={history} />}
      <NoDownloadNotice />
    </section>
  )
}

/** How long Safari's overlays take to go (their closing animations, safari.css), ms. */
const LEAVE_MS = 320

/** The tab's picture where its page shows, while the page itself doesn't (an overlay going over it). */
function PagePicture({ tab }: { tab: number }) {
  const picture = useSafari((state) => pictureOf(tab, state.pictures[tab]))
  return <div className="sf-page">{picture && <img className="sf-page-picture" src={picture} alt="" />}</div>
}

// ---------- The toolbar ----------

/** A round glass button of the toolbar's, alone or in a capsule with others. */
function BarButton({
  label,
  onClick,
  disabled = false,
  children,
}: {
  label: string
  onClick(): void
  disabled?: boolean
  children: ReactNode
}) {
  return (
    <button type="button" className="sf-button" aria-label={label} disabled={disabled} onClick={onClick}>
      {children}
    </button>
  )
}

/**
 * The toolbar. Separate: the sidebar button and Back, each its own circle (Forward beside Back once there's a page
 * to go forward to), the address field centred, New Tab and the tab overview in one capsule. Compact: the same, the
 * tabs in one capsule between them, the tab shown widened into its address field.
 */
function Toolbar({ tab, page, compact }: { tab: Tab | null; page: SafariTabState | undefined; compact: boolean }) {
  const text = useText()
  const typing = useSafari((state) => state.typing)
  const tabs = useSafari((state) => state.tabs)
  const pages = useSafari((state) => state.pages)
  return (
    <header className={`sf-toolbar${compact ? ' compact' : ''}${typing !== null ? ' typing' : ''}`}>
      <div className="sf-group">
        <BarButton label={text.safari.sidebar} onClick={() => useSafari.setState({ view: 'history', typing: null })}>
          <SidebarGlyph />
        </BarButton>
      </div>
      <div className={page?.forward ? 'sf-group sf-pair' : 'sf-group'}>
        <BarButton label={text.safari.back} disabled={!page?.back} onClick={back}>
          <BackGlyph />
        </BarButton>
        {page?.forward && (
          <BarButton label={text.safari.forward} onClick={forward}>
            <ForwardGlyph />
          </BarButton>
        )}
      </div>
      {!compact && (
        <div className={typing !== null ? 'sf-address-slot typing' : 'sf-address-slot'}>
          {typing !== null ? <TypingPanel /> : <AddressField tab={tab} page={page} />}
        </div>
      )}
      {compact && (
        <div className={typing !== null ? 'sf-compact-tabs typing' : 'sf-compact-tabs'}>
          {typing !== null && <TypingPanel />}
          {typing === null &&
            tabs.map((each) =>
              each.id === tab?.id ? (
                <AddressField key={each.id} tab={tab} page={page} compact />
              ) : (
                <button type="button" key={each.id} className="sf-compact-tab" onClick={() => selectTab(each.id)}>
                  <SiteIcon url={pages[each.id]?.url || each.url} favicon={pages[each.id]?.favicon || each.favicon} />
                  <span>{tabName(each, pages[each.id]) || text.safari.newTab}</span>
                </button>
              ),
            )}
        </div>
      )}
      <div className="sf-group sf-pair">
        <BarButton label={text.safari.newTab} onClick={newTab}>
          <PlusGlyph />
        </BarButton>
        <BarButton label={text.safari.showTabs} onClick={showTabs}>
          <TabsGlyph />
        </BarButton>
      </div>
    </header>
  )
}

/** The tab overview, a fresh picture of the tab shown in it. */
function showTabs(): void {
  const { active } = useSafari.getState()
  if (active !== null) send({ action: 'picture', tab: active })
  useSafari.setState({ view: 'tabs', typing: null })
}

/**
 * The address field while not typed into: the page menu at its left, the site alone (or Not Secure before it),
 * Reload at its right, a page loading drawn along its foot. In the Compact tab bar it's the tab shown: its close
 * button at its left, the site, then Reload and the page menu at its right. Pressed, it's typed into.
 */
function AddressField({
  tab,
  page,
  compact = false,
}: {
  tab: Tab | null
  page: SafariTabState | undefined
  compact?: boolean
}) {
  const text = useText()
  const [menu, setMenu] = useState(false)
  const menuShown = usePresence(menu, MENU_LEAVE_MS)
  const url = page?.url || tab?.url || ''
  const shown = url ? shownAddress(url) : null
  const startTyping = () => useSafari.setState({ typing: shown?.search ? shown.text : url })
  const menuButton = url && (
    <button type="button" className="sf-in-field" aria-label={text.safari.pageMenu} onClick={() => setMenu(!menu)}>
      <PageMenuGlyph />
    </button>
  )
  return (
    <div className={compact ? 'sf-address compact' : 'sf-address'}>
      {compact && tab && (
        <button
          type="button"
          className="sf-in-field"
          aria-label={text.safari.closeTab}
          onClick={() => closeTab(tab.id)}
        >
          <CloseGlyph />
        </button>
      )}
      {!compact && menuButton}
      <button type="button" className="sf-address-text" onClick={startTyping}>
        {!shown && (
          <span className="sf-hint">
            <SearchGlyph /> {text.safari.searchOrEnter}
          </span>
        )}
        {shown && (
          <>
            {!shown.secure && <span className="sf-not-secure">{text.safari.notSecure} —</span>}
            <span className="sf-site">{shown.text}</span>
          </>
        )}
      </button>
      {url && (
        <button
          type="button"
          className="sf-in-field"
          aria-label={page?.loading ? text.safari.stop : text.safari.reload}
          onClick={reloadOrStop}
        >
          {page?.loading ? <CloseGlyph /> : <ReloadGlyph />}
        </button>
      )}
      {compact && menuButton}
      {page?.loading && <span className="sf-progress" aria-hidden />}
      {menuShown && url && <PageMenu compact={compact} presence={menuShown} onClose={() => setMenu(false)} />}
    </div>
  )
}

/**
 * The address field typed into, as Safari's: the field and what it suggests in one glass panel over the page (no
 * Cancel: putting the keyboard away ends the typing). First the pages of History that match (each its title, its
 * address under it), then the search with the engine chosen under its heading ("Google Suggestions").
 */
function TypingPanel() {
  const text = useText()
  const typed = useSafari((state) => state.typing ?? '')
  const engine = useSafari((state) => state.engine)
  const history = useSafari((state) => state.history)
  const matches = useMemo(() => historyMatches(history, typed, 3), [history, typed])
  const typingHere = useKeyboard((state) => state.field?.id === ADDRESS_FIELD)
  const wasTyping = useRef(false)
  const panel = useRef<HTMLDivElement>(null)
  const words = typed.trim()
  useCover(panel, true)
  useEffect(() => {
    if (typingHere) wasTyping.current = true
    else if (wasTyping.current) useSafari.setState({ typing: null })
  }, [typingHere])
  // What's suggested fits between the field and the keyboard (it rises as the field is typed into).
  const [room, setRoom] = useState<number | null>(null)
  useEffect(() => {
    const measure = () => {
      const element = panel.current
      const keys = document.querySelector('.keyboard.shown')?.getBoundingClientRect()
      if (!element || !keys) return
      const box = element.getBoundingClientRect()
      const zoom = box.width / (element.offsetWidth || box.width)
      const field = element.querySelector('.text-field')?.getBoundingClientRect()
      const top = field ? field.bottom : box.top
      setRoom(Math.max(60, (keys.top - top) / zoom - 16))
    }
    measure()
    const timers = [120, 400].map((ms) => setTimeout(measure, ms))
    return () => timers.forEach(clearTimeout)
  }, [typingHere])
  return (
    <div
      ref={panel}
      className={words ? 'sf-typing-panel open' : 'sf-typing-panel'}
      style={room ? ({ '--sf-suggestions-room': `${room}px` } as CSSProperties) : undefined}
    >
      <TextField
        id={ADDRESS_FIELD}
        value={typed}
        onChange={(value) => useSafari.setState({ typing: value })}
        onSubmit={() => goTo(useSafari.getState().typing ?? '')}
        returnKind="go"
        hint={text.safari.searchOrEnter}
        label={text.safari.searchOrEnter}
        icon={words ? <GlobeGlyph /> : <SearchGlyph />}
        autoSelect
      />
      {words && (
        <div className="sf-suggestions" role="listbox">
          {matches.map((visit) => (
            <button type="button" key={visit.url} className="sf-suggestion hit" onClick={() => load(visit.url)}>
              <GlobeGlyph />
              <span className="sf-suggestion-text">
                <span className="sf-suggestion-title">{visit.title || siteOf(visit.url)}</span>
                <span className="sf-suggestion-site">{visit.url.replace(/^https?:\/\/(www\.)?/, '')}</span>
              </span>
            </button>
          ))}
          <div className="sf-suggestions-head">{text.safari.engineSuggestions(engineName(engine))}</div>
          <button type="button" className="sf-suggestion" onClick={() => goTo(words)}>
            <SearchGlyph />
            <span className="sf-suggestion-title">{words}</span>
          </button>
        </div>
      )}
    </div>
  )
}

const ADDRESS_FIELD = 'safari-address'

/** How long a menu takes to go (safari.css), ms. */
const MENU_LEAVE_MS = 200

/** The page menu, from its button in the address field: the page's text size, smaller or bigger (Safari's small
 * and large "A"), over the page. */
function PageMenu({ compact, presence, onClose }: { compact: boolean; presence: 'in' | 'out'; onClose(): void }) {
  const text = useText()
  const panel = useRef<HTMLDivElement>(null)
  useCover(panel, true)
  return (
    <>
      {presence === 'in' && <div className="sf-menu-away" onClick={onClose} aria-hidden />}
      <div ref={panel} className={`sf-menu${compact ? ' end' : ''} ${presence}`} role="menu">
        <div className="sf-zoom">
          <button
            type="button"
            className="sf-zoom-step small"
            aria-label={text.safari.zoomOut}
            onClick={() => zoom(-1)}
          >
            A
          </button>
          <span className="sf-zoom-divider" aria-hidden />
          <button type="button" className="sf-zoom-step" aria-label={text.safari.zoomIn} onClick={() => zoom(1)}>
            A
          </button>
        </div>
      </div>
    </>
  )
}

// ---------- The tab bar ----------

/** Separate's tab bar: the tabs on one rounded track under the toolbar, the tab shown raised on it (its highlight
 * sliding to the tab picked, by transform), its close button at its left. With one tab, none (as Safari). A tab
 * opened grows in; one closed shrinks away first. */
function TabBar() {
  const text = useText()
  const tabs = useSafari((state) => state.tabs)
  const active = useSafari((state) => state.active)
  const pages = useSafari((state) => state.pages)
  const closing = useClosing()
  const track = useRef<HTMLDivElement>(null)
  const [highlight, setHighlight] = useState<{ left: number; width: number } | null>(null)
  useLayoutEffect(() => {
    const element = track.current?.querySelector<HTMLElement>('.sf-tab.active')
    if (!element) return setHighlight(null)
    const place = () => setHighlight({ left: element.offsetLeft, width: element.offsetWidth })
    place()
    const resized = new ResizeObserver(place)
    resized.observe(element)
    return () => resized.disconnect()
  }, [active, tabs.length])
  return (
    <nav className="sf-tabbar">
      <div ref={track} className="sf-tabtrack">
        {highlight && (
          <span
            className="sf-tab-highlight"
            style={{ width: highlight.width, transform: `translateX(${highlight.left}px)` }}
            aria-hidden
          />
        )}
        {tabs.map((tab) => (
          <div
            key={tab.id}
            className={`sf-tab${tab.id === active ? ' active' : ''}${closing.has(tab.id) ? ' closing' : ''}`}
          >
            <button
              type="button"
              className="sf-tab-close"
              aria-label={text.safari.closeTab}
              onClick={() => closing.close(tab.id)}
            >
              <CloseGlyph />
            </button>
            <button type="button" className="sf-tab-name" onClick={() => selectTab(tab.id)}>
              <SiteIcon url={pages[tab.id]?.url || tab.url} favicon={pages[tab.id]?.favicon || tab.favicon} />
              <span>{tabName(tab, pages[tab.id]) || text.safari.newTab}</span>
            </button>
          </div>
        ))}
      </div>
    </nav>
  )
}

/** How long a tab takes to shrink away as it's closed (safari.css), ms. */
const CLOSE_MS = 220

/** Tabs being closed: each shrinks away first, then closes. */
function useClosing() {
  const [closing, setClosing] = useState<ReadonlySet<number>>(new Set())
  const timers = useRef<ReturnType<typeof setTimeout>[]>([])
  useEffect(() => () => timers.current.forEach(clearTimeout), [])
  return {
    has: (id: number) => closing.has(id),
    close: (id: number) => {
      setClosing((now) => new Set(now).add(id))
      timers.current.push(
        setTimeout(() => {
          closeTab(id)
          setClosing((now) => {
            const next = new Set(now)
            next.delete(id)
            return next
          })
        }, CLOSE_MS),
      )
    },
  }
}

/** A site's icon (the page's own, else its name's first letter on a tile). */
function SiteIcon({ url, favicon, large = false }: { url: string; favicon?: string; large?: boolean }) {
  const [broken, setBroken] = useState<string | null>(null)
  const site = siteOf(url)
  if (favicon && broken !== favicon) {
    return (
      <img className={large ? 'sf-icon large' : 'sf-icon'} src={favicon} alt="" onError={() => setBroken(favicon)} />
    )
  }
  return (
    <span className={large ? 'sf-icon letter large' : 'sf-icon letter'} aria-hidden>
      {site ? site[0].toUpperCase() : <CompassGlyph />}
    </span>
  )
}

// ---------- The page's places ----------

/** The Start Page of a new tab: the sites visited most (no Favourites: the owner's choice). */
function StartPage() {
  const text = useText()
  const history = useSafari((state) => state.history)
  const frequent = useMemo(() => frequentlyVisited(history, 8), [history])
  return (
    <div className="sf-start">
      {frequent.length > 0 && (
        <section className="sf-start-section">
          <h2>{text.safari.frequentlyVisited}</h2>
          <div className="sf-start-grid">
            {frequent.map((visit, at) => (
              <button
                type="button"
                key={visit.url}
                className="sf-start-site"
                style={{ '--at': at } as CSSProperties}
                onClick={() => load(visit.url)}
              >
                <SiteIcon url={visit.url} large />
                <span>{visit.title || siteOf(visit.url)}</span>
              </button>
            ))}
          </div>
        </section>
      )}
    </div>
  )
}

/** A page that couldn't open, as Safari says it. */
function ErrorPage({ tab, reason }: { tab: Tab; reason: string }) {
  const text = useText()
  const why =
    reason === 'host name not resolved'
      ? text.safari.serverNotFound
      : reason === 'disconnected'
        ? text.safari.notConnected
        : reason === 'timed out'
          ? text.safari.timedOut
          : reason === 'certificate refused'
            ? text.safari.notSecureConnection
            : text.safari.couldNotOpen
  return (
    <div className="sf-error">
      <CompassGlyph />
      <h2>{text.safari.cantOpen}</h2>
      <p>{why}</p>
      <button type="button" className="sf-error-retry" onClick={() => load(tab.url)}>
        {text.safari.tryAgain}
      </button>
    </div>
  )
}

/** The field pressed in the page, typed into by the cab's keyboard (and the player's own): what it types is told
 * the host as keys, against what the field held as it was pressed (a password's as dots). */
function WebField({ tab, page }: { tab: number; page: SafariTabState | undefined }) {
  const language = keyboardLanguage(useLanguage())
  const field = page?.field
  // A field pressed anew (`n`) is typed into anew, from its text as the page said it then.
  const pressed = field ? `${field.kind}:${field.n ?? 0}` : null
  const latest = useRef({ field, language })
  latest.current = { field, language }
  useEffect(() => {
    const { field, language } = latest.current
    if (!pressed || !field) return
    let mirror = field.value
    const typing: Typing = {
      id: `safari-page-${tab}`,
      returnKind: field.kind === 'search' ? 'search' : 'go',
      read: () => mirror,
      write: (next) => {
        for (const step of typedSteps(mirror, next)) send(step)
        mirror = next
      },
      submit: () => send({ action: 'key', key: 'enter' }),
    }
    useKeyboard.getState().attach(typing, language)
    return () => useKeyboard.getState().detach(typing.id)
  }, [tab, pressed])
  return null
}

/** A drop-down list pressed in the page, as iPad Safari shows one: its options in a menu under it (over it, where
 * there's no room under), the one picked ticked, its groups' names over theirs; one pressed is picked and the menu
 * goes. Pressed anywhere else, it goes (the page says so for a press on it: safari.js). The browser's own would be
 * a window on the PC's screen, not on the page. */
function PageList({ tab, list }: { tab: number; list: SafariList | undefined }) {
  const presence = usePresence(!!list, MENU_LEAVE_MS)
  const shown = useLast(list)
  if (!presence || !shown) return null
  return <ListMenu key={shown.id} tab={tab} list={shown} presence={presence} />
}

/** Room kept between the menu and the list's field, and the page's edges. */
const LIST_GAP = 8

function ListMenu({ tab, list, presence }: { tab: number; list: SafariList; presence: 'in' | 'out' }) {
  const panel = useRef<HTMLDivElement>(null)
  const chosen = useRef<HTMLButtonElement>(null)
  useCover(panel, true)
  // Where the list's field is, in Safari's own pixels (the host's are the frame's, from the page's top left).
  const [place, setPlace] = useState<CSSProperties | null>(null)
  const [above, setAbove] = useState(false)
  useLayoutEffect(() => {
    const content = panel.current?.parentElement
    if (!content) return
    const box = content.getBoundingClientRect()
    const zoom = box.width / (content.offsetWidth || box.width || 1)
    const scale = window.devicePixelRatio * zoom
    const [width, height] = [content.offsetWidth, content.offsetHeight]
    if (!list.rect) {
      setPlace({ left: '50%', top: '50%', translate: '-50% -50%', maxHeight: height - 2 * LIST_GAP })
      return
    }
    const field = {
      x: list.rect.x / scale,
      y: list.rect.y / scale,
      width: list.rect.width / scale,
      height: list.rect.height / scale,
    }
    const below = height - (field.y + field.height) - 2 * LIST_GAP
    const over = field.y - 2 * LIST_GAP
    const up = below < 240 && over > below
    setAbove(up)
    const left = Math.max(LIST_GAP, Math.min(field.x, width - LIST_GAP - 260))
    setPlace({
      left,
      minWidth: Math.max(220, Math.min(field.width, width - left - LIST_GAP)),
      maxWidth: width - left - LIST_GAP,
      ...(up
        ? { bottom: height - field.y + LIST_GAP, maxHeight: Math.max(120, over) }
        : { top: field.y + field.height + LIST_GAP, maxHeight: Math.max(120, below) }),
    })
  }, [list])
  // The one picked in sight as it opens.
  useLayoutEffect(() => {
    // (Not returned: it gives a promise now, which React would take for the effect's clean-up.)
    chosen.current?.scrollIntoView({ block: 'center' })
  }, [place])
  const close = (index?: number) => send({ action: 'pick', tab, list: list.id, index })
  return (
    <>
      {presence === 'in' && <div className="sf-list-away" onClick={() => close()} aria-hidden />}
      <div
        ref={panel}
        className={`sf-menu sf-list${above ? ' above' : ''} ${presence}`}
        style={place ?? { visibility: 'hidden' }}
        role="listbox"
      >
        {list.options.map((option, index) => {
          const group = option.group && option.group !== list.options[index - 1]?.group ? option.group : null
          const picked = index === list.selected
          return (
            <div key={index} className="sf-list-entry">
              {group && <div className="sf-list-group">{group}</div>}
              <button
                ref={picked ? chosen : undefined}
                type="button"
                className={`sf-list-option${picked ? ' picked' : ''}`}
                role="option"
                aria-selected={picked}
                disabled={option.disabled}
                onClick={() => close(index)}
              >
                <span className="sf-list-tick">{picked && <CheckGlyph />}</span>
                <span className="sf-list-label">{option.label}</span>
              </button>
            </div>
          )
        })}
      </div>
    </>
  )
}

/** A page's dialog, as Safari shows it: the site, the message, and its buttons (a prompt's text field). */
function PageDialog() {
  const asked = useSafari((state) => state.dialog)
  const presence = usePresence(asked !== null, MENU_LEAVE_MS)
  const dialog = useLast(asked)
  if (!presence || !dialog) return null
  return <DialogCard key={`${dialog.tab}:${dialog.dialog.message}`} dialog={dialog} presence={presence} />
}

function DialogCard({
  dialog,
  presence,
}: {
  dialog: NonNullable<ReturnType<typeof useSafari.getState>['dialog']>
  presence: 'in' | 'out'
}) {
  const text = useText()
  const tabs = useSafari((state) => state.tabs)
  const [typed, setTyped] = useState(() => dialog?.dialog.text ?? '')
  const panel = useRef<HTMLDivElement>(null)
  useCover(panel, true)
  const site = siteOf(tabs.find((tab) => tab.id === dialog.tab)?.url ?? '')
  return (
    <div className="sf-dialog-back">
      <div ref={panel} className={`sf-dialog ${presence}`} role="alertdialog">
        <div className="sf-dialog-text">
          <h3>{site}</h3>
          <p>{dialog.dialog.message}</p>
          {dialog.dialog.kind === 'prompt' && (
            <TextField
              id="safari-prompt"
              value={typed}
              onChange={setTyped}
              onSubmit={() => answer(true, typed)}
              hint=""
              label={site}
              autoSelect
            />
          )}
        </div>
        <div className="sf-dialog-buttons">
          {dialog.dialog.kind !== 'alert' && (
            <button type="button" onClick={() => answer(false)}>
              {text.safari.cancel}
            </button>
          )}
          <button type="button" className="default" onClick={() => answer(true, typed)}>
            {text.safari.ok}
          </button>
        </div>
      </div>
    </div>
  )
}

/** A page tried to download a file: said a moment, rising in and fading away. */
function NoDownloadNotice() {
  const text = useText()
  const at = useSafari((state) => state.noDownload)
  const presence = usePresence(at !== null, MENU_LEAVE_MS)
  useEffect(() => {
    if (at === null) return
    const timer = setTimeout(() => {
      if (useSafari.getState().noDownload === at) useSafari.setState({ noDownload: null })
    }, 3000)
    return () => clearTimeout(timer)
  }, [at])
  const panel = useRef<HTMLDivElement>(null)
  useCover(panel, presence !== null)
  if (!presence) return null
  return (
    <div ref={panel} className={`sf-notice ${presence}`} role="status">
      <WarningGlyph /> {text.safari.noDownloads}
    </div>
  )
}

// ---------- The tab overview ----------

/** The tab overview, as Safari's: the tabs' count in a glass capsule at the top, New Tab and Done (a blue tick) at
 * its right; the tabs' pictures in a grid, each its close button at its top right, its icon and title under it. */
function TabOverview({ presence }: { presence: 'in' | 'out' }) {
  const text = useText()
  const tabs = useSafari((state) => state.tabs)
  const active = useSafari((state) => state.active)
  const pages = useSafari((state) => state.pages)
  const pictures = useSafari((state) => state.pictures)
  const closing = useClosing()
  return (
    <div className={`sf-overview ${presence}`}>
      <header className="sf-overview-bar">
        <span />
        <div className="sf-capsule-title">{text.safari.tabCount(tabs.length)}</div>
        <div className="sf-overview-actions">
          <BarButton label={text.safari.newTab} onClick={newTab}>
            <PlusGlyph />
          </BarButton>
          <button
            type="button"
            className="sf-done"
            aria-label={text.safari.done}
            onClick={() => useSafari.setState({ view: 'page' })}
            disabled={tabs.length === 0}
          >
            <CheckGlyph />
          </button>
        </div>
      </header>
      <div className="sf-overview-grid">
        {tabs.map((tab, at) => {
          const picture = pictureOf(tab.id, pictures[tab.id])
          return (
            <div
              key={tab.id}
              className={`sf-card${tab.id === active ? ' active' : ''}${closing.has(tab.id) ? ' closing' : ''}`}
              style={{ '--at': Math.min(at, 8) } as CSSProperties}
            >
              <button type="button" className="sf-card-picture" onClick={() => selectTab(tab.id)}>
                {picture ? <img src={picture} alt="" /> : <span className="sf-card-blank" />}
              </button>
              <button
                type="button"
                className="sf-card-close"
                aria-label={text.safari.closeTab}
                onClick={() => closing.close(tab.id)}
              >
                <CloseGlyph />
              </button>
              <div className="sf-card-name">
                <SiteIcon url={pages[tab.id]?.url || tab.url} favicon={pages[tab.id]?.favicon || tab.favicon} />
                <span>{tabName(tab, pages[tab.id]) || text.safari.newTab}</span>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

// ---------- History ----------

const HOUR = 60 * 60 * 1000

/** History, as the sidebar shows it on an iPad: Back, its title, More (with Clear); its search; the pages visited
 * by day, the latest first. */
function HistoryView({ presence }: { presence: 'in' | 'out' }) {
  const text = useText()
  const history = useSafari((state) => state.history)
  const [search, setSearch] = useState('')
  const [more, setMore] = useState(false)
  const moreShown = usePresence(more, MENU_LEAVE_MS)
  const [clearing, setClearing] = useState(false)
  const clearShown = usePresence(clearing, LEAVE_MS)
  const days = useMemo(
    () => byDay(search ? historyMatches(history, search, 200) : [...history].reverse()),
    [history, search],
  )
  const [startOfToday] = useState(() => new Date().setHours(0, 0, 0, 0))
  const dayName = (day: number) =>
    day === startOfToday
      ? text.safari.today
      : day === new Date(startOfToday - HOUR * 12).setHours(0, 0, 0, 0)
        ? text.safari.yesterday
        : new Date(day).toLocaleDateString(text.locale, { weekday: 'long', day: 'numeric', month: 'long' })
  return (
    <div className={`sf-history ${presence}`}>
      <header className="sf-overview-bar">
        <BarButton label={text.safari.back} onClick={() => useSafari.setState({ view: 'page' })}>
          <BackGlyph />
        </BarButton>
        <h2>{text.safari.history}</h2>
        <div className="sf-overview-actions">
          <BarButton label={text.safari.more} onClick={() => setMore(!more)} disabled={history.length === 0}>
            <MoreGlyph />
          </BarButton>
          {moreShown && (
            <>
              {moreShown === 'in' && <div className="sf-menu-away" onClick={() => setMore(false)} aria-hidden />}
              <div className={`sf-menu end list ${moreShown}`} role="menu">
                <button
                  type="button"
                  className="sf-menu-item"
                  onClick={() => {
                    setMore(false)
                    setClearing(true)
                  }}
                >
                  {text.safari.clear}
                </button>
              </div>
            </>
          )}
        </div>
      </header>
      <div className="sf-history-search">
        <TextField
          id="safari-history"
          value={search}
          onChange={setSearch}
          hint={text.safari.searchHistory}
          label={text.safari.searchHistory}
          returnKind="search"
          icon={<SearchGlyph />}
        />
      </div>
      <div className="sf-history-list">
        {days.length === 0 && <div className="sf-history-empty">{text.safari.noHistory}</div>}
        {days.map(([day, visits]) => (
          <section key={day}>
            <h3>{dayName(day)}</h3>
            <div className="sf-history-card">
              {visits.map((visit) => (
                <button
                  type="button"
                  key={`${visit.at}-${visit.url}`}
                  className="sf-history-row"
                  onClick={() => {
                    if (useSafari.getState().active === null) newTab()
                    load(visit.url)
                  }}
                >
                  <SiteIcon url={visit.url} />
                  <span className="sf-history-title">{visit.title || siteOf(visit.url)}</span>
                  <span className="sf-history-site">{siteOf(visit.url)}</span>
                </button>
              ))}
            </div>
          </section>
        ))}
      </div>
      {clearShown && <ClearSheet presence={clearShown} onClose={() => setClearing(false)} />}
    </div>
  )
}

/** Visits (latest first) by the day they were on (its start, local time), the latest day first. */
function byDay(visits: readonly Visit[]): [number, Visit[]][] {
  const days = new Map<number, Visit[]>()
  for (const visit of visits) {
    const day = new Date(visit.at).setHours(0, 0, 0, 0)
    const list = days.get(day) ?? []
    list.push(visit)
    days.set(day, list)
  }
  return [...days.entries()]
}

/** Each clear timeframe's words. */
export function timeframeName(timeframe: ClearTimeframe, text: ReturnType<typeof useText>): string {
  return {
    lastHour: text.safari.lastHour,
    today: text.safari.todayOnly,
    todayAndYesterday: text.safari.todayAndYesterday,
    allHistory: text.safari.allHistory,
  }[timeframe]
}

/** Safari's Clear History sheet: its close button, the timeframe (a blue tick on the one chosen), Close All Tabs,
 * and the red Clear History. */
function ClearSheet({ presence, onClose }: { presence: 'in' | 'out'; onClose(): void }) {
  const text = useText()
  const count = useSafari((state) => state.tabs.length)
  const [timeframe, setTimeframe] = useState<ClearTimeframe>('lastHour')
  const [closeTabs, setCloseTabs] = useState(false)
  return (
    <div className={`sf-dialog-back blocking ${presence}`} onClick={onClose}>
      <div className={`sf-sheet ${presence}`} role="dialog" onClick={(event) => event.stopPropagation()}>
        <header className="sf-sheet-head">
          <button type="button" className="sf-button" aria-label={text.safari.cancel} onClick={onClose}>
            <CloseGlyph />
          </button>
          <h3>{text.safari.clearHistory}</h3>
          <span />
        </header>
        <h4>{text.safari.clearTimeframe}</h4>
        <div className="sf-sheet-card">
          {CLEAR_TIMEFRAMES.map((choice) => (
            <button
              type="button"
              key={choice}
              role="radio"
              aria-checked={timeframe === choice}
              className="sf-sheet-row"
              onClick={() => setTimeframe(choice)}
            >
              <span>{timeframeName(choice, text)}</span>
              {timeframe === choice && <CheckGlyph />}
            </button>
          ))}
        </div>
        <h4>{text.safari.additionalOptions}</h4>
        <div className="sf-sheet-card">
          <button
            type="button"
            role="switch"
            aria-checked={closeTabs}
            className="sf-sheet-row"
            onClick={() => setCloseTabs(!closeTabs)}
          >
            <span>{text.safari.closeAllTabs}</span>
            <span className={closeTabs ? 'apple-switch on' : 'apple-switch'} aria-hidden>
              <span className="apple-switch-thumb" />
            </span>
          </button>
        </div>
        <p className="sf-sheet-footer">{text.safari.closesTabs(count)}</p>
        <button
          type="button"
          className="sf-sheet-clear"
          onClick={() => {
            clearHistory(timeframeStart(timeframe, Date.now()), closeTabs)
            onClose()
          }}
        >
          {text.safari.clearHistory}
        </button>
      </div>
    </div>
  )
}
