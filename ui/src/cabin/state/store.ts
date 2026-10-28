import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import {
  type Equalizer,
  FLAT,
  type HostMessage,
  type GameWeather,
  hasTruck,
  type JobInfo,
  IS_DEMO,
  type KeybindAction,
  isInHost,
  postToHost,
  subscribeHostMessages,
  type Telemetry,
  toEqualizer,
  type TruckInfo,
} from '../../shared/host'
import type { Maneuver, TurnKind } from '../maps/guidance'
import {
  receiveAnswer as receiveAppleMusicAnswer,
  receivePlayback as receiveAppleMusicPlayback,
  useAppleMusic,
} from '../apps/music/appleMusic'
import {
  receiveAnswer as receiveSpotifyAnswer,
  receivePlayback,
  receiveState as receiveSpotifyState,
} from '../apps/spotify/spotify'
import { receiveAnswer, receiveState, useYouTube } from '../apps/youtube/youtube'
import {
  receiveAnswer as receiveYouTubeMusicAnswer,
  receivePlayback as receiveYouTubeMusicPlayback,
  receiveState as receiveYouTubeMusicState,
} from '../apps/youtubeMusic/youtubeMusic'
import { useKeyboard } from '../keyboard/store'
import { receiveMapFolder } from '../maps/mapData'
import { receiveDetours, receiveFollowed, receiveRouteAnswer, receiveRoutes } from '../maps/routeChange'
import type { Point, RouteNode } from '../maps/route'
import { DEFAULT_WALLPAPER_ID } from '../wallpapers'
import { isReleasedApp } from './apps'
import type { AppearanceMode } from './appearance'
import { afterRunningTransition, animateNavigation, launchFor, type LaunchOrigin } from './launch'
import {
  INITIAL_NAVIGATION,
  navigate,
  type NavigationAction,
  type NavigationState,
  type Route,
  routeKey,
} from './navigation'
import { isIconStyle, setIcons, useLooks } from './looks'
import type { OemMode } from './oem'
import { INITIAL_POWER, power, type PowerEvent, type PowerState } from './power'
import { isScreenLayoutId, type ScreenLayoutId } from './screen'
import { DEFAULT_TEXT_SIZE, STEPS_ADDED_BELOW, toTextSize } from './textSize'

/** The Dashboard's widgets the player can turn off (turn-by-turn always shows while there's a route). */
export type DashboardWidget = 'nowPlaying' | 'destination' | 'weather'

export const DASHBOARD_WIDGETS: readonly DashboardWidget[] = ['nowPlaying', 'destination', 'weather']

/**
 * Cabin state that the UI renders from. Telemetry lives in its own store (`useLive`) because it updates
 * up to 20 times a second; components subscribe to just the fields they need.
 */
interface CabinState {
  navigation: NavigationState
  appearanceMode: AppearanceMode
  wallpaperId: string
  /** The Dashboard's widgets shown (Settings, Dashboard). */
  dashboardWidgets: Record<DashboardWidget, boolean>
  /** Album art on Now Playing and in Music (Settings, Now Playing): off, a plain note instead. */
  albumArt: boolean
  /** The apps' text size, a step of `TEXT_SIZES` (Settings › Appearance › Text Size). */
  textSize: number
  /** The truck's own strip: kept (`auto`) or not, as set in the companion window (the host sends it). */
  oemMode: OemMode
  /** The player's own tone curve (Settings › Equalizer), and whether it plays (Custom) or the speakers as tuned
   * (Recommended, flat): kept with WestPlay's settings, the host sends them. */
  equalizer: Equalizer
  equalizerCustom: boolean
  power: PowerState
  /**
   * The screen is handed back to the truck (its stock screen shows, with WestPlay's button to come back),
   * like CarPlay's car button. Not kept: WestPlay comes back with every power-up, like CarPlay reconnecting.
   */
  truckScreen: boolean
  navigate(action: NavigationAction): void
  setAppearanceMode(mode: AppearanceMode): void
  setWallpaper(id: string): void
  setDashboardWidget(widget: DashboardWidget, shown: boolean): void
  setAlbumArt(shown: boolean): void
  setTextSize(step: number): void
  /** One band of the curve, set in Settings › Equalizer (choosing Custom): played by every player, and kept with
   * WestPlay's settings. */
  setEqualizerBand(band: keyof Equalizer, db: number): void
  /** Custom (the curve plays) or Recommended (flat), the curve kept either way. */
  chooseEqualizer(custom: boolean): void
  /** The curve flat again. */
  resetEqualizer(): void
  setOemMode(mode: OemMode): void
  /** The player's choice, in Settings: applied, and kept with WestPlay's settings. */
  chooseOemMode(mode: OemMode): void
  powerEvent(event: PowerEvent): void
  setTruckScreen(shown: boolean): void
}

/** What survives a restart of the app: where you were and your choices, never the power state. */
type Persisted = Pick<
  CabinState,
  'navigation' | 'appearanceMode' | 'wallpaperId' | 'dashboardWidgets' | 'albumArt' | 'textSize'
>

const ALL_WIDGETS: Record<DashboardWidget, boolean> = { nowPlaying: true, destination: true, weather: true }

export const useCabin = create<CabinState>()(
  persist(
    (set) => ({
      navigation: INITIAL_NAVIGATION,
      appearanceMode: 'auto',
      wallpaperId: DEFAULT_WALLPAPER_ID,
      dashboardWidgets: ALL_WIDGETS,
      albumArt: true,
      textSize: DEFAULT_TEXT_SIZE,
      oemMode: 'auto',
      equalizer: FLAT,
      equalizerCustom: false,
      power: INITIAL_POWER,
      navigate: (action) => set((state) => ({ navigation: navigate(state.navigation, action) })),
      setAppearanceMode: (appearanceMode) => set({ appearanceMode }),
      setWallpaper: (id) => {
        set({ wallpaperId: id })
        // Kept with WestPlay's settings, where the companion's Customize sees it.
        if (isInHost()) postToHost({ type: 'wallpaper', id })
      },
      setDashboardWidget: (widget, shown) =>
        set((state) => ({ dashboardWidgets: { ...state.dashboardWidgets, [widget]: shown } })),
      setAlbumArt: (albumArt) => set({ albumArt }),
      setTextSize: (step) => set({ textSize: toTextSize(step) }),
      setEqualizerBand: (band, db) => {
        const equalizer = toEqualizer({ ...useCabin.getState().equalizer, [band]: db })
        set({ equalizer, equalizerCustom: true })
        if (isInHost()) postToHost({ type: 'equalizer', custom: true, ...equalizer })
      },
      chooseEqualizer: (custom) => {
        set({ equalizerCustom: custom })
        if (isInHost()) postToHost({ type: 'equalizer', custom, ...useCabin.getState().equalizer })
      },
      resetEqualizer: () => {
        set({ equalizer: FLAT })
        if (isInHost()) postToHost({ type: 'equalizer', custom: useCabin.getState().equalizerCustom, ...FLAT })
      },
      setOemMode: (oemMode) => set({ oemMode }),
      chooseOemMode: (oemMode) => {
        set({ oemMode })
        if (isInHost()) postToHost({ type: 'truckStrip', strip: oemMode })
      },
      powerEvent: (event) =>
        set((state) => {
          const next = power(state.power, event)
          return { power: next, truckScreen: next.phase === 'off' ? false : state.truckScreen }
        }),
      truckScreen: false,
      setTruckScreen: (truckScreen) => set({ truckScreen }),
    }),
    {
      // The onboarding's demo keeps its own: it never changes the player's.
      name: IS_DEMO ? 'westplay.cabin.demo' : 'westplay.cabin',
      // 2: Text Size's steps moved larger (2026-10-24): a step kept from before means another size now, so each
      // player starts on the new default. 3: two smaller steps added below them (the same day): a step kept is the
      // same size two steps further on.
      version: 3,
      migrate: (stored, version) => {
        const kept = (stored ?? {}) as Partial<Persisted>
        if (version < 2) return { ...kept, textSize: DEFAULT_TEXT_SIZE }
        if (version < 3 && typeof kept.textSize === 'number') {
          return { ...kept, textSize: kept.textSize + STEPS_ADDED_BELOW }
        }
        return kept
      },
      storage: createJSONStorage(() => localStorage),
      partialize: (state): Persisted => ({
        navigation: state.navigation,
        appearanceMode: state.appearanceMode,
        wallpaperId: state.wallpaperId,
        dashboardWidgets: state.dashboardWidgets,
        albumArt: state.albumArt,
        textSize: state.textSize,
      }),
      // Stored data comes from an earlier version of WestPlay (or was edited): keep only what is valid.
      merge: (stored, current) => ({ ...current, ...sanitize(stored as Partial<Persisted> | undefined) }),
    },
  ),
)

function sanitize(stored: Partial<Persisted> | undefined): Partial<Persisted> {
  if (!stored) return {}
  const result: Partial<Persisted> = {}
  if (stored.navigation && isRoute(stored.navigation.route) && Array.isArray(stored.navigation.recents)) {
    const { route, recents, base } = stored.navigation
    result.navigation = {
      route,
      recents: recents.filter(isReleasedApp),
      // Saved before WestPlay remembered it: the screen shown, or the Dashboard (where it starts).
      base: base === 'home' || base === 'dashboard' ? base : route.kind === 'home' ? 'home' : 'dashboard',
    }
  }
  if (stored.appearanceMode === 'auto' || stored.appearanceMode === 'dark' || stored.appearanceMode === 'light') {
    result.appearanceMode = stored.appearanceMode
  }
  // Any id: one of the player's wallpapers is only known once the host lists them (one gone shows the default).
  if (typeof stored.wallpaperId === 'string') result.wallpaperId = stored.wallpaperId
  if (typeof stored.dashboardWidgets === 'object' && stored.dashboardWidgets !== null) {
    const widgets = stored.dashboardWidgets as Partial<Record<DashboardWidget, unknown>>
    result.dashboardWidgets = Object.fromEntries(
      DASHBOARD_WIDGETS.map((widget) => [widget, widgets[widget] !== false]),
    ) as Record<DashboardWidget, boolean>
  }
  if (typeof stored.albumArt === 'boolean') result.albumArt = stored.albumArt
  if (stored.textSize !== undefined) result.textSize = toTextSize(stored.textSize)
  return result
}

function isRoute(route: unknown): route is Route {
  if (typeof route !== 'object' || route === null) return false
  const { kind, app } = route as { kind?: unknown; app?: unknown }
  return kind === 'home' || kind === 'dashboard' || (kind === 'app' && isReleasedApp(app))
}

/** Live data from the game. */
export interface LiveState {
  telemetry: Telemetry | null
  truck: TruckInfo | null
  /** The player's job (`null`: none). */
  job: JobInfo | null
  /** The kind of screen the game draws WestPlay on; `null` until it has drawn one. */
  screenLayout: ScreenLayoutId | null
  /** The cities whose garages the player owns, and their headquarters' (none until the host has read their
   * save). */
  ownedGarages: string[]
  headquarters: string | null
  /** The player plays on a map by HVT, which WestPlay doesn't allow: the game's language to say so in.
   * `null`: not (or not known yet). */
  hvtMap: { language: string | null } | null
  /** The game's language (its `g_lang_init`, e.g. `tr_tr`; `null` until the host has read it). */
  gameLanguage: string | null
  /** The player's profile has the game's time zones on (its `g_time_zones`): the clock is the time where
   * the truck is (state/timeZone.ts). */
  timeZones: boolean
  /** How far the clock where the truck is runs from the game's time, minutes: its country's time zone with
   * the profile's time zones on, else 0 (state/timeZone.ts). Every clock shown adds it (`cabClock`). */
  clockShift: number
  /** The route the game's GPS plans (`null`: none): its map nodes, each one's distance left to the
   * destination (as the game counts distances), the ways through its junctions and the turns there (by the
   * node each starts at, once the host has found them), how far right of its way it runs at its nodes on
   * roads (on the carriageway it drives on), and the node ahead of the truck along it. */
  route: {
    id: number
    nodes: RouteNode[]
    distances: number[]
    paths: ReadonlyMap<number, Point[]>
    maneuvers: Maneuver[]
    shifts: ReadonlyMap<number, number>
  } | null
  routeNext: number
  /** The player's stops along the route: their map nodes' indices in it, ascending. */
  routeStops: readonly number[]
  /** The game's weather (`null`: not known). */
  weather: GameWeather | null
  /** Whether the game's window shows, in front or not (no game: as if it did). */
  gameShown: boolean
  /** The game's menu gone while it's still paused (the music comes back then, not 1.5 s later). */
  menuGone: boolean
  /** The keybind last pressed in the game, and how many so far (each press a new one). */
  keybind: { action: KeybindAction; count: number } | null
  /** Whether the game runs under TruckersMP. */
  truckersmp: boolean
  /** Under TruckersMP, the players around the player from its SDK, as the host last said (`null`: none). */
  nearby: Nearby | null
  /** The music's spectrum as the host last said it (Settings › Equalizer's, `HostMessage` `spectrum`). */
  spectrum: readonly number[]
  /** The host draws the screen's cursor (`cursor` message): the page draws none of its own then. */
  cursorByHost: boolean
}

/** TruckersMP's players around the player, from its SDK: its game server's id, the player's own number on it,
 * the others (number, world x and z, heading in turns), and when it came (page clock, ms). */
export interface Nearby {
  server: number
  local: number
  players: [id: number, x: number, z: number, heading: number][]
  at: number
}

export const useLive = create<LiveState>()(() => ({
  telemetry: null,
  truck: previewTruck(),
  job: null,
  screenLayout: previewLayout(),
  ownedGarages: [],
  headquarters: null,
  hvtMap: null,
  gameLanguage: null,
  timeZones: false,
  clockShift: 0,
  route: null,
  routeNext: 0,
  routeStops: [],
  weather: null,
  gameShown: true,
  menuGone: false,
  keybind: null,
  truckersmp: false,
  nearby: null,
  spectrum: [],
  cursorByHost: false,
}))

/** Plain browser previews have no game to report the screen: `?layout=scania-2025` stands in for it. */
/** In a browser, `?truck=scania.s_2016` drives that truck (its screen profile). */
function previewTruck(): TruckInfo | null {
  if (isInHost()) return null
  const model = new URLSearchParams(location.search).get('truck')
  if (!model) return null
  const brandId = model.split('.')[0]
  return { brandId, brand: brandId, id: `vehicle.${model}`, name: model }
}

function previewLayout(): ScreenLayoutId | null {
  if (isInHost()) return null
  const layout = new URLSearchParams(location.search).get('layout')
  return isScreenLayoutId(layout) ? layout : null
}

/**
 * Feeds host messages into the stores, and tells the host whether WestPlay is open on the truck's screen
 * (as the page connects, and whenever it opens or closes). Returns the unsubscribe function; call once at
 * startup. The power sequence follows the truck's electrics (and only changes when they change).
 */
export function connectHost(now: () => number = () => performance.now()): () => void {
  const stopMessages = subscribeHostMessages(receiveHostMessage(now))
  const report = () => postToHost({ type: 'westplay', open: !useCabin.getState().truckScreen })
  report()
  const stopReports = useCabin.subscribe((state, previous) => {
    if (state.truckScreen !== previous.truckScreen) report()
  })
  // The field selected, for the player's own keyboard to type into (the host passes it to the game plugin).
  const reportSelection = () => postToHost({ type: 'typing', selection: useKeyboard.getState().selection })
  reportSelection()
  const stopSelections = useKeyboard.subscribe((state, previous) => {
    if (state.selection !== previous.selection) reportSelection()
  })
  return () => {
    stopMessages()
    stopReports()
    stopSelections()
  }
}

function receiveHostMessage(now: () => number): (message: HostMessage) => void {
  return (message) => {
    switch (message.type) {
      case 'telemetry': {
        const previous = useLive.getState().telemetry
        useLive.setState({ telemetry: message.data })
        if (previous?.electricEnabled !== message.data.electricEnabled) {
          // The tablet isn't the truck's screen: no brand as it starts.
          const truckScreen = useLive.getState().screenLayout !== 'tablet'
          useCabin
            .getState()
            .powerEvent({ type: 'electrics', on: message.data.electricEnabled, at: now(), truckScreen })
        }
        break
      }
      case 'truck':
        useLive.setState({ truck: hasTruck(message.data) ? message.data : null })
        break
      case 'job':
        useLive.setState({ job: message.data.active ? message.data : null })
        break
      case 'garages':
        useLive.setState({ ownedGarages: message.owned, headquarters: message.headquarters })
        break
      case 'routeAnswer':
        receiveRouteAnswer(message.id, message.answer)
        break
      case 'routes':
        receiveRoutes(message.id, message.routes)
        break
      case 'followed':
        receiveFollowed(message.id, message.follows, message.answer)
        break
      case 'detours':
        receiveDetours(message.id, message.seconds)
        break
      case 'typed':
        for (const event of message.events) useKeyboard.getState().typed(event)
        break
      case 'typingState':
        useKeyboard.setState({ physical: message.state })
        break
      case 'mapBlock':
        useLive.setState({
          hvtMap: message.hvt ? { language: message.language } : null,
          gameLanguage: message.language,
        })
        break
      case 'timeZones':
        useLive.setState({ timeZones: message.on })
        break
      case 'route':
        useLive.setState({
          route:
            message.id === null || message.nodes === null
              ? null
              : {
                  id: message.id,
                  nodes: message.nodes.map(([x, z, fx, fz]) => ({ position: [x, z], forward: [fx, fz] })),
                  distances: message.nodes.map((node) => node[4]),
                  paths: new Map(),
                  maneuvers: [],
                  shifts: new Map(),
                },
          routeNext: 0,
          routeStops: [],
        })
        break
      case 'routeStops':
        if (useLive.getState().route?.id === message.id) useLive.setState({ routeStops: message.stops })
        break
      case 'routePaths': {
        const route = useLive.getState().route
        if (route?.id !== message.id) break
        const maneuvers = message.maneuvers.map(([node, turn, angle, exit]) => ({
          node,
          turn: turn as TurnKind,
          angle,
          exit,
        }))
        const shifts = new Map(message.shifts ?? [])
        useLive.setState({ route: { ...route, paths: new Map(message.paths), maneuvers, shifts } })
        break
      }
      case 'routeNext':
        useLive.setState({ routeNext: message.next })
        break
      case 'weather':
        useLive.setState({ weather: message.weather })
        break
      case 'screen':
        // A layout this UI doesn't know yet (newer plugin) is treated like a screen without a profile.
        useLive.setState({ screenLayout: isScreenLayoutId(message.layout) ? message.layout : null })
        break
      case 'settings': {
        const cabin = useCabin.getState()
        cabin.setOemMode(message.truckStrip === 'off' ? 'off' : 'auto')
        if (message.wallpaper) useCabin.setState({ wallpaperId: message.wallpaper })
        // Chosen here before WestPlay kept it with its settings: kept now.
        else if (cabin.wallpaperId !== DEFAULT_WALLPAPER_ID) postToHost({ type: 'wallpaper', id: cabin.wallpaperId })
        useCabin.setState({
          equalizer: toEqualizer(message.equalizer),
          equalizerCustom: message.equalizerCustom === true,
        })
        setIcons({
          iconSet: message.iconSet ?? null,
          iconStyle: isIconStyle(message.iconStyle) ? message.iconStyle : 'auto',
        })
        break
      }
      case 'spectrum':
        useLive.setState({ spectrum: message.bars })
        break
      case 'cursor':
        useLive.setState({ cursorByHost: message.host })
        break
      case 'customize':
        useLooks.setState({ custom: { wallpapers: message.wallpapers, iconSets: message.iconSets } })
        break
      case 'westplay':
        useCabin.getState().setTruckScreen(!message.open)
        break
      case 'youtube':
        receiveState(message.state)
        break
      case 'gameShown':
        useLive.setState({ gameShown: message.shown })
        break
      case 'menuGone':
        useLive.setState({ menuGone: message.gone })
        break
      case 'keybind':
        useLive.setState((live) => ({ keybind: { action: message.action, count: (live.keybind?.count ?? 0) + 1 } }))
        break
      case 'truckersmp':
        useLive.setState({ truckersmp: message.on, ...(message.on ? {} : { nearby: null }) })
        break
      case 'nearby':
        useLive.setState({
          nearby:
            message.server === null || message.local === null
              ? null
              : { server: message.server, local: message.local, players: message.players, at: now() },
        })
        break
      case 'game':
        // The game closed: the screen goes off as with the truck's electrics (the kept apps go with it), and
        // the next game's first telemetry powers it on afresh.
        if (!message.running) {
          useLive.setState({ telemetry: null })
          useCabin.getState().powerEvent({ type: 'electrics', on: false, at: now() })
        }
        break
      case 'youtubeAccount':
        useYouTube.setState({ account: { signedIn: message.signedIn, avatar: message.avatar } })
        break
      case 'youtubeSignIn':
        useYouTube.setState({ signingIn: message.open })
        break
      case 'map':
        receiveMapFolder(message.folder, message.built)
        break
      case 'spotify':
        receiveSpotifyState(message.signedIn, message.playing, message.signingIn)
        break
      case 'spotifyAnswer':
        receiveSpotifyAnswer(message)
        break
      case 'spotifyPlayback':
        receivePlayback(message.playback)
        break
      case 'appleMusic':
        useAppleMusic.setState({
          signedIn: message.signedIn,
          playing: message.playing,
          signingIn: message.signingIn,
          avatar: message.avatar,
        })
        break
      case 'appleMusicAnswer':
        receiveAppleMusicAnswer(message)
        break
      case 'appleMusicPlayback':
        receiveAppleMusicPlayback(message.playback)
        break
      case 'youtubeMusic':
        receiveYouTubeMusicState(message)
        break
      case 'youtubeMusicAnswer':
        receiveYouTubeMusicAnswer(message)
        break
      case 'youtubeMusicPlayback':
        receiveYouTubeMusicPlayback(message.playback)
        break
      case 'youtubeAnswer': {
        const { type: _, id, ...answer } = message
        receiveAnswer(id, answer)
        break
      }
    }
  }
}

let latestNavigation = 0

/**
 * Navigates the way a player does, animated: an app grows out of the icon that opened it (`origin`) and
 * shrinks back into its Home or sidebar icon when it closes; Home and the Dashboard trade places
 * (state/launch.ts).
 *
 * Like a touchscreen, every press counts and the last one is what you see: a press during an animation
 * finishes it, presses overtaken by a later one apply at once, and the last one animates.
 */
export function go(action: NavigationAction, origin: LaunchOrigin | null = null): void {
  const id = ++latestNavigation
  afterRunningTransition(() => {
    const cabin = useCabin.getState()
    const before = cabin.navigation.route
    const after = navigate(cabin.navigation, action).route
    // Overtaken by a later press, nothing to change, or Home and the Dashboard trading places (animated live
    // by CabinApp's `Screens`): no View Transition.
    const betweenBases = before.kind !== 'app' && after.kind !== 'app'
    if (id !== latestNavigation || routeKey(after) === routeKey(before) || betweenBases) {
      cabin.navigate(action)
      return
    }
    animateNavigation(launchFor(before, after, origin), () => useCabin.getState().navigate(action))
  })
}
