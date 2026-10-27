import type { Customizations, IconStyle } from '../shared/customize'
import { IS_DEMO } from '../shared/host'
import { showMapAt } from './maps/mapData'
import type { AppId } from './state/apps'
import { setIcons, useLooks } from './state/looks'
import { buttonAt } from './shell/clickThrough'
import { routeKey } from './state/navigation'
import { go, useCabin, useLive } from './state/store'
import './demo.css'

/**
 * The cabin page as a live demo, in the companion's onboarding (`cabin.html?demo&power`, in an iframe of the
 * companion's own page): the real WestPlay, driven by the onboarding instead of the game. It powers on (the
 * WestPlay intro), goes where it's told (Home, the Dashboard, an app), shows the map the onboarding names,
 * and says where an app's icon is, for the onboarding's cursor to move there and tap it.
 *
 * It shows the player's wallpaper and icons as the companion tells it (`looks`), is always dark, and keeps its
 * state apart from the player's (state/store.ts).
 */
export type DemoCommand =
  /** On: the WestPlay intro, then the screen. Off: black. */
  | { type: 'power'; on: boolean }
  | { type: 'go'; to: 'home' | 'dashboard' | AppId }
  /** Where something on the screen is (`target`, a CSS selector: an app's icon on Home,
   * `.app[data-app="music"] img`), answered with `located`. */
  | { type: 'locate'; target: string; id: number }
  /** Presses something on the screen (`target`, a CSS selector), as a player does. */
  | { type: 'tap'; target: string }
  /** Presses what's at `x`, `y` (the screen's own pixels, 1280x720): the player's own click, in the
   * tutorial (the companion keeps the keyboard, to know the key they hold). */
  | { type: 'press'; x: number; y: number }
  | { type: 'map'; base: string | null; built: number | null }
  /** The companion's window isn't in front (`away`): every animation holds still until it is again. */
  | { type: 'away'; away: boolean }
  /** The language it speaks: a game language code (`tr_tr`), as the game would say. */
  | { type: 'language'; code: string }
  /** The player's wallpapers and icon sets (their files at `base`, the companion's address for them), and the
   * wallpaper and icon set chosen (`null`: none chosen, WestPlay's own). */
  | {
      type: 'looks'
      custom: Customizations
      base: string
      wallpaper: string | null
      iconSet: string | null
      iconStyle: IconStyle
    }

export type DemoReport =
  | { type: 'ready' }
  /** The icon's box in the screen's own pixels (1280x720), `null` if it isn't on screen. */
  | { type: 'located'; id: number; rect: { x: number; y: number; width: number; height: number } | null }
  /** Where it is now (`home`, `dashboard`, `app:music`), each time that changes. */
  | { type: 'route'; route: string }

/** Marks demo messages apart from anything else posted between the windows. */
export const DEMO_CHANNEL = 'westplay-demo'

export function installDemo(): void {
  if (!IS_DEMO) return
  document.documentElement.classList.add('demo')
  // Part of the companion's window, so no right-click menu either (companion/main.tsx).
  document.addEventListener('contextmenu', (event) => event.preventDefault())
  const cabin = useCabin.getState()
  cabin.setAppearanceMode('dark')
  // Starts off, on Home, until the onboarding powers it on.
  useCabin.setState({
    power: { phase: 'off', since: performance.now(), observed: true },
    navigation: { ...cabin.navigation, route: { kind: 'home' }, base: 'home' },
  })

  const report = (message: DemoReport) =>
    window.parent.postMessage({ channel: DEMO_CHANNEL, ...message }, location.origin)
  window.addEventListener('message', (event: MessageEvent) => {
    if (event.origin !== location.origin || event.source !== window.parent) return
    const data = event.data as { channel?: string } & DemoCommand
    if (data?.channel !== DEMO_CHANNEL) return
    switch (data.type) {
      case 'power': {
        const now = performance.now()
        // On: straight to the WestPlay intro (the phone connecting), as when WestPlay starts mid-drive.
        useCabin.setState({ power: { phase: data.on ? 'connecting' : 'off', since: now, observed: true } })
        break
      }
      case 'go': {
        const route = useCabin.getState().navigation.route
        // Where to, not which button: told twice, it's still there (the Home button, pressed on Home, would
        // go on to the Dashboard).
        if (data.to === 'home') {
          go({ type: 'showHome' })
        } else if (data.to === 'dashboard') {
          go({ type: 'showDashboard' })
        } else {
          go({ type: 'openApp', app: data.to }, route.kind === 'home' ? 'home' : 'sidebar')
        }
        break
      }
      case 'locate': {
        const box = document.querySelector(data.target)?.getBoundingClientRect()
        report({
          type: 'located',
          id: data.id,
          rect: box && box.width > 0 ? { x: box.x, y: box.y, width: box.width, height: box.height } : null,
        })
        break
      }
      case 'tap':
        document.querySelector<HTMLElement>(data.target)?.click()
        break
      case 'press': {
        // What's there; during a screen's animation (its view transition covers the page), the button there
        // by layout, as the cab's own clicks do then (shell/clickThrough.ts).
        const at = document.elementFromPoint(data.x, data.y)?.closest<HTMLElement>('button, a, [role="button"], .app')
        ;(at ?? buttonAt(data.x, data.y))?.click()
        break
      }
      case 'map':
        showMapAt(data.base, data.built)
        break
      case 'away':
        document.documentElement.toggleAttribute('data-away', data.away)
        break
      case 'language':
        useLive.setState({ gameLanguage: data.code })
        break
      case 'looks':
        useLooks.setState({ custom: data.custom, base: data.base })
        setIcons({ iconSet: data.iconSet, iconStyle: data.iconStyle })
        // None chosen: the demo's own (above).
        if (data.wallpaper) useCabin.setState({ wallpaperId: data.wallpaper })
        break
    }
  })
  report({ type: 'ready' })
  let route = routeKey(useCabin.getState().navigation.route)
  report({ type: 'route', route })
  useCabin.subscribe((state) => {
    const now = routeKey(state.navigation.route)
    if (now === route) return
    route = now
    report({ type: 'route', route })
  })
}
