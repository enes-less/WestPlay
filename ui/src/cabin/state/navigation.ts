import type { AppId } from './apps'

/** Where the cabin screen is. */
export type Route = { kind: 'home' } | { kind: 'dashboard' } | { kind: 'app'; app: AppId }

/** The two screens apps are opened from. */
export type BaseScreen = 'home' | 'dashboard'

export interface NavigationState {
  route: Route
  /** Most recently used apps, newest first; the sidebar shows them. */
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

/** The sidebar has room for three recent apps. */
export const RECENTS_LIMIT = 3

export const INITIAL_NAVIGATION: NavigationState = {
  route: { kind: 'dashboard' },
  recents: ['maps', 'music', 'youtube'],
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
