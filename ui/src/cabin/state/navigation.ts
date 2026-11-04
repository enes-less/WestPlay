import { APP_IDS, type AppId, isReleasedApp } from './apps'

/** Where the cabin screen is. */
export type Route = { kind: 'home' } | { kind: 'dashboard' } | { kind: 'app'; app: AppId }

/** The two screens apps are opened from. */
export type BaseScreen = 'home' | 'dashboard'

export interface NavigationState {
  route: Route
  /** The apps in the order last used, newest first; the sidebar shows each kind's last ([`sidebarApps`]). */
  recents: AppId[]
  /** Home or the Dashboard, whichever was shown last: where the Home button leaves an app for. */
  base: BaseScreen
}

export type NavigationAction =
  | { type: 'openApp'; app: AppId }
  /** The sidebar's Home button, which behaves like CarPlay's (see `navigate`). */
  | { type: 'pressHome' }
  | { type: 'showDashboard' }
  /** Home itself (the onboarding's demo: cabin/demo.ts), wherever the screen is; nothing if it's there. */
  | { type: 'showHome' }

/** The apps kept in the order last used: all of them, so each sidebar slot knows its last used app however
 * many others were opened since. */
export const RECENTS_LIMIT = APP_IDS.length

/**
 * The sidebar's slots, as CarPlay's (owner 2026-11-03): not the last opened apps but one app of each kind, the
 * last used of it: a maps app, a music app, Phone or Messages, a calendar. Each slot's apps, its first shown
 * until another is used. A slot whose apps this release hasn't is left out.
 */
const SIDEBAR_SLOTS: readonly (readonly AppId[])[] = [
  ['maps'],
  ['music', 'spotify', 'youtubeMusic', 'player', 'podcasts'],
  ['phone', 'messages'],
  ['calendar'],
]

/** At most this many: the fourth (a calendar) only where the screen has room (`ScreenProfile.recents`). */
export const SIDEBAR_SLOT_COUNT = SIDEBAR_SLOTS.length

/** The sidebar's apps: each slot's last used (`recents`, newest first) or its first, at most `room`. */
export function sidebarApps(recents: readonly AppId[], room: number): AppId[] {
  const apps: AppId[] = []
  for (const slot of SIDEBAR_SLOTS) {
    const released = slot.filter(isReleasedApp)
    if (released.length === 0) continue
    apps.push(recents.find((app) => released.includes(app)) ?? released[0])
  }
  return apps.slice(0, Math.min(room, SIDEBAR_SLOT_COUNT))
}

export const INITIAL_NAVIGATION: NavigationState = {
  route: { kind: 'dashboard' },
  recents: ['maps', 'music', 'youtube', 'spotify', 'youtubeMusic'],
  base: 'dashboard',
}

/**
 * Where the Home button goes, after CarPlay's: from an app back to Home or the Dashboard, whichever it was
 * opened from; between those two, to the other. Its icon shows this destination.
 */
export function homeButtonTarget(state: NavigationState): BaseScreen {
  switch (state.route.kind) {
    case 'app':
      return state.base
    case 'home':
      return 'dashboard'
    case 'dashboard':
      return 'home'
  }
}

/** Navigation rules, after CarPlay. Opening an app moves it to the front of the recents. */
export function navigate(state: NavigationState, action: NavigationAction): NavigationState {
  switch (action.type) {
    case 'openApp':
      return {
        ...state,
        route: { kind: 'app', app: action.app },
        recents: [action.app, ...state.recents.filter((app) => app !== action.app)].slice(0, RECENTS_LIMIT),
      }
    case 'pressHome': {
      const target = homeButtonTarget(state)
      return { ...state, route: { kind: target }, base: target }
    }
    case 'showDashboard':
      return { ...state, route: { kind: 'dashboard' }, base: 'dashboard' }
    case 'showHome':
      return { ...state, route: { kind: 'home' }, base: 'home' }
  }
}

/** A stable string for a route, e.g. for React keys. */
export function routeKey(route: Route): string {
  return route.kind === 'app' ? `app:${route.app}` : route.kind
}
