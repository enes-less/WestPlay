import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import { isInHost, postToHost, type SafariCommand, type SafariDialog, type SafariTabState } from '../../../shared/host'
import { addressFor, DEFAULT_ENGINE, isSearchEngine, type SearchEngine, siteOf } from './address'

/**
 * Safari (owner 2026-11-07): the web in the cab, in iPad Safari's look. Each tab's page is the host's
 * (`westplay_host::safari`), shown under the cab where the app leaves room for it (WebPage.tsx); this keeps
 * what Safari keeps: the open tabs, the history and the search engine (the owner's choices: no favourites), and
 * follows each page as the host says it is. A site's sign-in and cookies are the host's browser's own.
 */

/** A tab as kept: its page's address and title, and its icon's address. */
export interface Tab {
  id: number
  url: string
  title: string
  favicon?: string
}

/** A page visited, for History: its address, title and when (ms since 1970). */
export interface Visit {
  url: string
  title: string
  at: number
}

/** The tab bar's layouts, as an iPad's Safari offers them (iPadOS 26.4): Separate, its default, and Compact. */
export type TabBarLayout = 'separate' | 'compact'

/** The most visits History keeps (the oldest go first). */
export const HISTORY_MOST = 2000

/** Where the host numbers a tab a page opened from (`westplay_host::safari::OPENED_FROM`): the cab's are under it. */
const OPENED_FROM = 2 ** 30

/** A page's zoom, as Safari's page menu offers it. */
export const ZOOMS = [0.5, 0.75, 0.85, 1, 1.15, 1.25, 1.5, 1.75, 2, 2.5, 3] as const

interface SafariState {
  /** The open tabs, in the tab bar's order. */
  tabs: Tab[]
  /** The tab shown (`null`: none open, the Start Page of a new one). */
  active: number | null
  history: Visit[]
  engine: SearchEngine
  /** Each site's zoom chosen in the page menu (1: as the page is). */
  zooms: Record<string, number>
  /** The tab bar's layout (Settings › Safari › Tabs, as an iPad's): Separate (the tabs in a bar of their own, under
   * the toolbar) or Compact (the tabs in the toolbar, the one shown its address field). */
  tabBar: TabBarLayout
  /** Show Color in Tab Bar (Settings › Safari › Tabs): the bars in the page's own colour. */
  pageColour: boolean
  nextId: number
  // Not kept:
  /** Each live tab's page, as the host said it last. */
  pages: Record<number, SafariTabState>
  /** Each tab's picture's version (the tab overview's), as the host wrote it. */
  pictures: Record<number, number>
  /** A page's dialog waiting for the player. */
  dialog: { tab: number; dialog: SafariDialog } | null
  /** Whether any page plays sound (the host says). */
  playing: boolean
  /** Safari's views over the page: the tab overview, History, the page menu. */
  view: 'page' | 'tabs' | 'history'
  /** The address field typed into (its text as typed), `null` when it isn't. */
  typing: string | null
  /** A page tried to download a file: said a moment. */
  noDownload: number | null
}

const IN_HOST = typeof window !== 'undefined' && isInHost()

export const useSafari = create<SafariState>()(
  persist(
    (): SafariState => ({
      tabs: [],
      active: null,
      history: [],
      engine: DEFAULT_ENGINE,
      zooms: {},
      tabBar: 'separate',
      pageColour: true,
      nextId: 1,
      pages: {},
      pictures: {},
      dialog: null,
      playing: false,
      view: 'page',
      typing: null,
      noDownload: null,
    }),
    {
      name: 'westplay.safari',
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({
        tabs: state.tabs,
        active: state.active,
        history: state.history,
        engine: state.engine,
        zooms: state.zooms,
        tabBar: state.tabBar,
        pageColour: state.pageColour,
        nextId: state.nextId,
      }),
      merge: (kept, current) => {
        const stored = (kept ?? {}) as Partial<SafariState>
        const found = Array.isArray(stored.tabs) ? stored.tabs.filter(isTab) : []
        // A tab a page opened was numbered by the host (`OPENED_FROM` on), which numbers them anew as it starts:
        // the cab's own number for it now.
        let nextId = Math.max(
          Number(stored.nextId) || 1,
          ...found.filter((tab) => tab.id < OPENED_FROM).map((tab) => tab.id + 1),
        )
        let active = found.some((tab) => tab.id === stored.active) ? (stored.active as number) : (found[0]?.id ?? null)
        const tabs = found.map((tab) => {
          if (tab.id < OPENED_FROM) return tab
          const id = nextId++
          if (active === tab.id) active = id
          return { ...tab, id }
        })
        return {
          ...current,
          tabs,
          active,
          history: Array.isArray(stored.history) ? stored.history.filter(isVisit).slice(-HISTORY_MOST) : [],
          engine: isSearchEngine(stored.engine) ? stored.engine : DEFAULT_ENGINE,
          zooms: stored.zooms && typeof stored.zooms === 'object' ? stored.zooms : {},
          tabBar: stored.tabBar === 'compact' ? 'compact' : 'separate',
          pageColour: stored.pageColour !== false,
          nextId,
        }
      },
    },
  ),
)

function isTab(value: unknown): value is Tab {
  const tab = value as Tab
  return !!tab && typeof tab.id === 'number' && typeof tab.url === 'string' && typeof tab.title === 'string'
}

function isVisit(value: unknown): value is Visit {
  const visit = value as Visit
  return !!visit && typeof visit.url === 'string' && typeof visit.title === 'string' && typeof visit.at === 'number'
}

export function send(command: SafariCommand): void {
  if (IN_HOST) postToHost({ type: 'safari', command })
}

/** The tab shown, as kept. */
export function activeTab(state: Pick<SafariState, 'tabs' | 'active'>): Tab | null {
  return state.tabs.find((tab) => tab.id === state.active) ?? null
}

/** A tab's name as its tab shows it: the page's title, else its site, else nothing (a new tab: the Start Page). */
export function tabName(tab: Tab, page?: SafariTabState): string {
  const title = page?.title || tab.title
  const url = page?.url || tab.url
  if (title && title !== url && !url.endsWith(title)) return title
  return siteOf(url) || title
}

/** A new tab, shown, on the Start Page; its address field typed into, as Safari's new tab is. */
export function newTab(): void {
  const { tabs, nextId, active } = useSafari.getState()
  const at = tabs.findIndex((tab) => tab.id === active)
  const tab: Tab = { id: nextId, url: '', title: '' }
  const next = [...tabs]
  next.splice(at < 0 ? next.length : at + 1, 0, tab)
  useSafari.setState({ tabs: next, active: tab.id, nextId: nextId + 1, view: 'page', typing: '' })
}

export function selectTab(id: number): void {
  useSafari.setState({ active: id, view: 'page', typing: null })
}

/** Closes a tab: the one next to it shown (the one after it, else before), as Safari does. */
export function closeTab(id: number): void {
  const { tabs, active, pages, pictures } = useSafari.getState()
  const at = tabs.findIndex((tab) => tab.id === id)
  if (at < 0) return
  const rest = tabs.filter((tab) => tab.id !== id)
  const nextActive = active === id ? (rest[at]?.id ?? rest[at - 1]?.id ?? null) : active
  const { [id]: _page, ...otherPages } = pages
  const { [id]: _picture, ...otherPictures } = pictures
  useSafari.setState({ tabs: rest, active: nextActive, pages: otherPages, pictures: otherPictures })
  send({ action: 'close', tab: id })
}

export function closeAllTabs(): void {
  for (const tab of useSafari.getState().tabs) send({ action: 'close', tab: tab.id })
  useSafari.setState({ tabs: [], active: null, pages: {}, pictures: {} })
}

/** Goes where the player typed (a web address, else a search) in the tab shown: a new tab if none is. */
export function goTo(typed: string): void {
  const url = addressFor(typed, useSafari.getState().engine)
  if (!url) return
  if (useSafari.getState().active === null) newTab()
  load(url)
}

/** Loads `url` in the tab shown. */
export function load(url: string): void {
  const { active, tabs } = useSafari.getState()
  if (active === null) return
  useSafari.setState({
    tabs: tabs.map((tab) => (tab.id === active ? { ...tab, url } : tab)),
    typing: null,
    view: 'page',
  })
  send({ action: 'open', tab: active, url })
}

export function back(): void {
  const { active } = useSafari.getState()
  if (active !== null) send({ action: 'back', tab: active })
}

export function forward(): void {
  const { active } = useSafari.getState()
  if (active !== null) send({ action: 'forward', tab: active })
}

/** Reload, or stop while it loads. */
export function reloadOrStop(): void {
  const { active, pages } = useSafari.getState()
  if (active === null) return
  send({ action: pages[active]?.loading ? 'stop' : 'reload', tab: active })
}

export function setEngine(engine: SearchEngine): void {
  useSafari.setState({ engine })
}

export function setTabBar(tabBar: TabBarLayout): void {
  useSafari.setState({ tabBar })
}

export function setPageColour(pageColour: boolean): void {
  useSafari.setState({ pageColour })
}

/** The zoom of the tab shown's site (1 unless chosen). */
export function zoomOf(state: Pick<SafariState, 'zooms'>, url: string): number {
  return state.zooms[siteOf(url)] ?? 1
}

/** The tab shown's site zoomed a step in (`1`) or out (`-1`), or back to as it is (`0`). */
export function zoom(step: 1 | -1 | 0): void {
  const state = useSafari.getState()
  const tab = activeTab(state)
  const site = tab && siteOf(state.pages[tab.id]?.url || tab.url)
  if (!site) return
  const now = zoomOf(state, `https://${site}/`)
  const at = ZOOMS.findIndex((z) => z >= now - 0.001)
  const next = step === 0 ? 1 : ZOOMS[Math.min(ZOOMS.length - 1, Math.max(0, (at < 0 ? 3 : at) + step))]
  const { [site]: _, ...others } = state.zooms
  useSafari.setState({ zooms: next === 1 ? others : { ...others, [site]: next } })
}

/** Safari's Clear History (Settings › Safari › Clear History and Website Data, History's Clear): the history from
 * `since` (ms since 1970; 0: all of it) on, and the websites' data from then (their cookies: signed out of them);
 * every tab closed too, if asked (its Close All Tabs). */
export function clearHistory(since: number, closeTabs: boolean): void {
  const history = since > 0 ? useSafari.getState().history.filter((visit) => visit.at < since) : []
  useSafari.setState({ history })
  send({ action: 'forgetWebsites', since: Math.max(0, since) })
  if (closeTabs) closeAllTabs()
}

/** The clear choices' timeframes, as Safari's sheet has them: when each starts (ms since 1970; 0: all of it). */
export type ClearTimeframe = 'lastHour' | 'today' | 'todayAndYesterday' | 'allHistory'

export const CLEAR_TIMEFRAMES: readonly ClearTimeframe[] = ['lastHour', 'today', 'todayAndYesterday', 'allHistory']

export function timeframeStart(timeframe: ClearTimeframe, now: number): number {
  const today = new Date(now).setHours(0, 0, 0, 0)
  switch (timeframe) {
    case 'lastHour':
      return now - 60 * 60 * 1000
    case 'today':
      return today
    case 'todayAndYesterday':
      return new Date(today - 12 * 60 * 60 * 1000).setHours(0, 0, 0, 0)
    case 'allHistory':
      return 0
  }
}

/** The answer to a page's dialog. */
export function answer(accept: boolean, text?: string): void {
  const { dialog } = useSafari.getState()
  if (!dialog) return
  useSafari.setState({ dialog: null })
  send({ action: 'answer', tab: dialog.tab, accept, ...(text !== undefined ? { text } : {}) })
}

/** The sites visited most, for the Start Page's Frequently Visited: each by its latest visit, at most `most`. */
export function frequentlyVisited(history: readonly Visit[], most: number): Visit[] {
  const sites = new Map<string, { count: number; latest: Visit }>()
  for (const visit of history) {
    const site = siteOf(visit.url)
    if (!site) continue
    const seen = sites.get(site)
    sites.set(site, { count: (seen?.count ?? 0) + 1, latest: visit })
  }
  return [...sites.values()]
    .sort((a, b) => b.count - a.count || b.latest.at - a.latest.at)
    .slice(0, most)
    .map((site) => site.latest)
}

/** The history's visits matching what's typed (their title or address), latest first, at most `most`. */
export function historyMatches(history: readonly Visit[], typed: string, most: number): Visit[] {
  const words = typed.trim().toLowerCase()
  if (!words) return []
  const seen = new Set<string>()
  const found: Visit[] = []
  for (let i = history.length - 1; i >= 0 && found.length < most; i--) {
    const visit = history[i]
    if (seen.has(visit.url)) continue
    if (visit.title.toLowerCase().includes(words) || visit.url.toLowerCase().includes(words)) {
      seen.add(visit.url)
      found.push(visit)
    }
  }
  return found
}

/** A page visited (its address loaded, not failed): into History, or the visit there updated with its title. */
export function visited(history: readonly Visit[], url: string, title: string, at: number): Visit[] {
  if (!/^https?:/i.test(url)) return [...history]
  const last = history.at(-1)
  if (last && last.url === url) {
    return last.title === title || !title ? [...history] : [...history.slice(0, -1), { ...last, title }]
  }
  return [...history, { url, title, at }].slice(-HISTORY_MOST)
}

// ---------- What the host says ----------

/** A tab's page as the host says it now. */
export function receiveTab(id: number, page: SafariTabState): void {
  const state = useSafari.getState()
  if (!state.tabs.some((tab) => tab.id === id)) {
    // A tab the cab no longer has (closed meanwhile): its page goes.
    send({ action: 'close', tab: id })
    return
  }
  const tabs = state.tabs.map((tab) =>
    tab.id === id && page.url && page.url !== 'about:blank'
      ? { ...tab, url: page.url, title: page.title, ...(page.favicon ? { favicon: page.favicon } : {}) }
      : tab,
  )
  const before = state.pages[id]
  const history =
    !page.loading &&
    !page.failed &&
    page.url &&
    (before?.url !== page.url || before?.title !== page.title || before?.loading)
      ? visited(state.history, page.url, page.title, Date.now())
      : state.history
  useSafari.setState({ tabs, history, pages: { ...state.pages, [id]: page } })
}

/** A page opened a new window: its own tab, after the page's, shown. */
export function receiveOpened(id: number, opener: number): void {
  const { tabs } = useSafari.getState()
  if (tabs.some((tab) => tab.id === id)) return
  const at = tabs.findIndex((tab) => tab.id === opener)
  const next = [...tabs]
  next.splice(at < 0 ? next.length : at + 1, 0, { id, url: '', title: '' })
  useSafari.setState({ tabs: next, active: id, view: 'page', typing: null })
}

/** A page closed its own window: its tab goes, back to the one it came from. */
export function receiveClosed(id: number): void {
  closeTab(id)
}

/** A tab's page let go: shown again, it loads from its address. */
export function receiveLetGo(id: number): void {
  const { [id]: _, ...pages } = useSafari.getState().pages
  useSafari.setState({ pages })
}

export function receiveDialog(tab: number, dialog: SafariDialog): void {
  useSafari.setState({ dialog: { tab, dialog }, active: tab, view: 'page' })
}

export function receivePicture(tab: number, version: number): void {
  useSafari.setState((state) => ({ pictures: { ...state.pictures, [tab]: version } }))
}

export function receiveNoDownload(): void {
  useSafari.setState({ noDownload: Date.now() })
}

export function receivePlaying(playing: boolean): void {
  useSafari.setState({ playing })
}

/** Where a tab's picture is (the host's `safari::TABS_HOST`), or `null` with none yet. */
export function pictureOf(tab: number, version: number | undefined): string | null {
  return version ? `https://westplay.safari-tabs/tab-${tab}.png?v=${version}` : null
}
