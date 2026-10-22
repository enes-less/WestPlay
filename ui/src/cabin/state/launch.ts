import type { CSSProperties } from 'react'
import { flushSync } from 'react-dom'
import { create } from 'zustand'
import type { AppId } from './apps'
import type { Route } from './navigation'

/**
 * App launch and close animations, like iOS: an app grows out of the icon that opened it and shrinks back
 * into its Home icon when it closes. Built on the View Transitions API: the browser snapshots the screen
 * before and after a navigation and animates between them; elements that carry the same transition name in
 * both snapshots morph into each other (the icon into the app window, or back). The motion is in
 * shell/transitions.css.
 */

/** Where the player clicked to open an app. */
export type LaunchOrigin = 'home' | 'sidebar'

/** An app window animating between an icon and its full size. */
export interface Launch {
  app: AppId
  /** The icon it grows out of (`open`) or shrinks into (`close`). */
  origin: LaunchOrigin
  direction: 'open' | 'close'
}

/** What a navigation animates; `null` for a plain crossfade (Home and Dashboard, for one). */
export function launchFor(before: Route, after: Route, origin: LaunchOrigin | null): Launch | null {
  if (after.kind === 'app' && origin !== null && !(before.kind === 'app' && before.app === after.app)) {
    return { app: after.app, origin, direction: 'open' }
  }
  // Closing to Home, the app shrinks into its Home icon; to the Dashboard, into its sidebar icon.
  if (before.kind === 'app' && after.kind !== 'app') {
    return { app: before.app, origin: after.kind === 'home' ? 'home' : 'sidebar', direction: 'close' }
  }
  return null
}

/**
 * The kind of screen change, for its motion (`html[data-transition]` in shell/transitions.css): an app
 * opening or closing, or a crossfade. (Home and the Dashboard trade places without one: CabinApp's
 * `Screens`.)
 */
export type TransitionKind = 'open' | 'close' | 'switch'

export function transitionKind(launch: Launch | null): TransitionKind {
  return launch?.direction ?? 'switch'
}

/** The transition name shared by the morphing icon and app window. */
const APP_WINDOW = 'app-window'

/** An element that can take part in a launch: an app's icon (on Home or in the sidebar), or its window. */
export type LaunchRole = { kind: 'icon'; origin: LaunchOrigin } | { kind: 'window' }

interface LaunchState {
  launch: Launch | null
  /** Which snapshot is being prepared: before the navigation (`from`) or after it (`to`). */
  phase: 'from' | 'to'
}

const useLaunch = create<LaunchState>()(() => ({ launch: null, phase: 'from' }))

/**
 * Whether this element is the morphing one in the snapshot being prepared. Opening, the icon is it before
 * the navigation and the window after; closing, the other way round. Only one element may carry a name in
 * each snapshot.
 */
export function carriesLaunch(state: LaunchState, app: AppId, role: LaunchRole): boolean {
  const { launch, phase } = state
  if (!launch || launch.app !== app) return false
  const iconPhase = launch.direction === 'open' ? 'from' : 'to'
  if (role.kind === 'icon') return role.origin === launch.origin && phase === iconPhase
  return phase !== iconPhase
}

/** Whether this element is the morphing one in the snapshot being prepared (see `carriesLaunch`). */
export function useCarriesLaunch(app: AppId, role: LaunchRole): boolean {
  return useLaunch((state) => carriesLaunch(state, app, role))
}

/** Style for an icon or app window: the shared transition name while it takes part in a launch. */
export function useLaunchStyle(app: AppId, role: LaunchRole): CSSProperties | undefined {
  return useCarriesLaunch(app, role) ? LAUNCH_STYLE : undefined
}

/** What the morphing element wears. */
export const LAUNCH_STYLE: CSSProperties = { viewTransitionName: APP_WINDOW }

/** Corner radius of the icon an app grows out of or shrinks into (Home icons are bigger). */
const ICON_RADIUS: Record<LaunchOrigin, string> = { home: '31px', sidebar: '17px' }

let running: ViewTransition | null = null

/**
 * Calls `next` once no screen change is animating any more: a running one is finished at once (it jumps to
 * its end), and `next` runs after its navigation has been applied, so it sees the current screen. Several
 * calls run in order.
 */
export function afterRunningTransition(next: () => void): void {
  if (!running) {
    next()
    return
  }
  running.skipTransition()
  running.updateCallbackDone.catch(() => {}).finally(next)
}

/** Calls `next` once the screen change animating now (if any) has finished, without hurrying it. */
export function whenTransitionEnds(next: () => void): void {
  if (!running) {
    next()
    return
  }
  running.finished.catch(() => {}).finally(next)
}

/**
 * Runs `update` (a navigation) as an animated transition. `update` must change the screen
 * synchronously. Without View Transitions support it just runs. Start it from `afterRunningTransition`, so
 * it begins from the current screen.
 */
export function animateNavigation(launch: Launch | null, update: () => void): void {
  if (typeof document.startViewTransition !== 'function') {
    update()
    return
  }
  running?.skipTransition()

  const root = document.documentElement
  flushSync(() => useLaunch.setState({ launch, phase: 'from' }))
  root.dataset.transition = transitionKind(launch)
  if (launch) root.style.setProperty('--launch-icon-radius', ICON_RADIUS[launch.origin])
  if (launch?.direction === 'close' && launch.origin === 'sidebar') intoSidebar(root)

  const transition = document.startViewTransition(() =>
    flushSync(() => {
      useLaunch.setState({ phase: 'to' })
      update()
    }),
  )
  running = transition
  transition.finished.finally(() => {
    if (running !== transition) return
    running = null
    delete root.dataset.transition
    delete root.dataset.intoSidebar
    root.style.removeProperty('--launch-icon-radius')
    root.style.removeProperty('--launch-window-cut')
    useLaunch.setState({ launch: null, phase: 'from' })
  })
}

/**
 * Closing into a sidebar icon, the window shrinks over the sidebar (shell/transitions.css), so it lands on
 * its icon in front, never behind the glass. App windows reach behind the sidebar, so that strip of it is
 * cut off (`--launch-window-cut`, a share of its width): at full size it doesn't cover the sidebar, only
 * what was beside it shows. Measured on the screen as it is before the change.
 */
function intoSidebar(root: HTMLElement): void {
  // The app showing (a slot put away is none).
  const app = document.querySelector('.screen-slot:not(.away) .screen.app-window')?.getBoundingClientRect()
  const sidebar = document.querySelector('.sidebar')?.getBoundingClientRect()
  if (!app || !sidebar || app.width === 0) return
  const cut = Math.min(0.5, Math.max(0, (sidebar.right - app.left) / app.width))
  root.dataset.intoSidebar = ''
  root.style.setProperty('--launch-window-cut', `${(cut * 100).toFixed(2)}%`)
}

/**
 * Runs `update` (a change of appearance) as one crossfade of the whole screen, the way iOS changes it: every
 * colour, the map and the wallpaper fade together, on Apple's curve (shell/transitions.css). A screen
 * change already animating keeps going, and the appearance just changes under it.
 */
export function animateAppearance(update: () => void): void {
  const root = document.documentElement
  if (typeof document.startViewTransition !== 'function' || (running && root.dataset.transition !== 'appearance')) {
    update()
    return
  }
  running?.skipTransition()

  root.dataset.transition = 'appearance'
  const transition = document.startViewTransition(() => flushSync(update))
  running = transition
  transition.finished.finally(() => {
    if (running !== transition) return
    running = null
    delete root.dataset.transition
  })
}
