import { createContext, useContext, useEffect } from 'react'
import type { DemoCommand } from '../../cabin/demo'
import type { AppId } from '../../cabin/state/apps'
import type { Banner } from './TruckScreen'
import type { Rect } from './useDemo'

/* The truck's screen in the room, as the pages use it: where it goes, what it shows, and the cursor's tours
   over it. Shared by the onboarding and the companion (moved from the onboarding, as it was). */

/** The light the screen casts, by what's on it. */
export const GLOW = {
  boot: 'oklch(0.72 0.1 250)',
  home: 'oklch(0.6 0.17 285)',
  scan: 'oklch(0.65 0.17 245)',
  maps: 'oklch(0.7 0.13 165)',
  music: 'oklch(0.62 0.21 15)',
  youtube: 'oklch(0.58 0.22 28)',
  spotify: 'oklch(0.66 0.17 150)',
  weather: 'oklch(0.66 0.13 232)',
  dashboard: 'oklch(0.64 0.15 240)',
}

/** What the steps do with the screen. */
export interface Screen {
  send(command: DemoCommand): void
  /** Takes the screen to `route`, the way the step wants it (once: see `go`). */
  go(route: 'home' | 'dashboard' | AppId): void
  locate(target: string): Promise<Rect | null>
  ready: boolean
  banner(banner: Banner | null): void
  scan(on: boolean): void
  glow(colour: string): void
  cursor(cursor: { x: number; y: number; pressing: boolean } | null): void
  /** Powers the screen off and on again: the WestPlay intro plays. */
  reboot(): void
  powered: boolean
  leaving: boolean
  /** How long until the screen has (nearly) finished moving to its new place, ms (0: it's there). */
  settling(): number
}

export const ScreenContext = createContext<Screen | null>(null)

export function useScreen(): Screen {
  const screen = useContext(ScreenContext)
  if (!screen) throw new Error('outside the onboarding')
  return screen
}

/** Puts the screen where a step wants it: on `route`, with `banner`, casting `glow`. */
export function useScreenAt(route: 'home' | 'dashboard' | AppId, glow: string, banner: Banner | null) {
  const screen = useScreen()
  const bannerKey = JSON.stringify(banner)
  useEffect(() => {
    // Once the screen is in its place.
    const timer = window.setTimeout(() => {
      screen.go(route)
      screen.glow(glow)
    }, screen.settling())
    return () => window.clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [route, glow, screen.ready])
  useEffect(() => {
    screen.banner(banner)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bannerKey])
}

/** How far the tours have zoomed the map, in steps (each zoom-in tap +1): the map keeps its zoom between
 * visits to its step, and a tour left halfway would leave it zoomed out, the next one zooming out further. */
let mapZoom = 0

/** What the cursor presses on the screen (CSS selectors in WestPlay's page). */
export const ON_SCREEN = {
  app: (app: AppId) => `.app[data-app="${app}"]`,
  icon: (app: AppId) => `.app[data-app="${app}"] img`,
  homeButton: '.sidebar-button.home-button',
  page: (page: number) => `.home-dot:nth-child(${page})`,
  zoomIn: '.map-control[data-control="zoom-in"]',
  zoomOut: '.map-control[data-control="zoom-out"]',
}

/** A press on the screen: the cursor moves to `on` (else `tap`), presses `tap`, the screen casts `glow`,
 * and it stays a while (`then`, ms). */
export interface Press {
  tap: string
  on?: string
  glow?: string
  then?: number
}

/**
 * The cursor shows the screen being used, over and over while the step is up: it moves to each press's
 * target, presses it, and the real WestPlay answers (an app opens, Home turns its page, the map zooms).
 * Presses whose target isn't on the screen are passed over.
 */
export function useTour(presses: Press[] | null, startAfter = 1200) {
  const screen = useScreen()
  const key = presses ? JSON.stringify(presses) : ''
  useEffect(() => {
    if (!presses) return
    let active = true
    const wait = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms))
    const run = async () => {
      await wait(startAfter)
      // A map zoomed by a tour cut short goes back to where it was before this one starts, once what it
      // presses is on the screen.
      if (presses.some((press) => press.tap === ON_SCREEN.zoomIn || press.tap === ON_SCREEN.zoomOut)) {
        while (active && mapZoom !== 0) {
          screen.send({ type: 'tap', target: mapZoom < 0 ? ON_SCREEN.zoomIn : ON_SCREEN.zoomOut })
          mapZoom += mapZoom < 0 ? 1 : -1
          await wait(350)
        }
      }
      let last = ''
      while (active) {
        for (const press of presses) {
          if (!active) return
          const target = press.on ?? press.tap
          const rect = await screen.locate(target)
          if (!active) return
          if (!rect) {
            await wait(400)
            continue
          }
          const at = { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 }
          screen.cursor({ ...at, pressing: false })
          // Across the screen takes a moment; pressing the same thing again, hardly any.
          await wait(target === last ? 260 : 1100)
          last = target
          if (!active) return
          screen.cursor({ ...at, pressing: true })
          await wait(150)
          if (!active) return
          screen.send({ type: 'tap', target: press.tap })
          if (press.tap === ON_SCREEN.zoomIn) mapZoom += 1
          if (press.tap === ON_SCREEN.zoomOut) mapZoom -= 1
          if (press.glow) screen.glow(press.glow)
          screen.cursor({ ...at, pressing: false })
          await wait(press.then ?? 1800)
        }
      }
    }
    run()
    return () => {
      active = false
      screen.cursor(null)
    }
    // Keyed by the presses.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key])
}

/** An app opened from Home, and back to Home through the sidebar's Home button. */
export function openAndClose(app: AppId, glow: string): Press[] {
  return [
    { tap: ON_SCREEN.app(app), on: ON_SCREEN.icon(app), glow, then: 2800 },
    { tap: ON_SCREEN.homeButton, glow: GLOW.home, then: 1500 },
  ]
}
