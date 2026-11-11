import { routeKey } from './navigation'
import { useCabin, useLive } from './store'

/** A frame this long (ms) or longer counts as slow: the page waited (for the processor or the graphics card). */
const SLOW_MS = 50

/** What the page says with each of the host's performance lines (`westplayReport()`). */
export interface FrameReport {
  /** Its animation frames since the last line. */
  frames: number
  /** Of them, those that came [`SLOW_MS`] or more after the one before. */
  slow: number
  /** The seconds each screen was shown (`maps`, `home`, `dashboard`, `off`, `truck`…). */
  screens: Record<string, number>
}

/** The screen the cab shows now, by name (an app by its own: `maps`). */
export function screenName(): string {
  const { power, truckScreen, navigation } = useCabin.getState()
  if (power.phase !== 'on') return 'off'
  if (truckScreen) return 'truck'
  return routeKey(navigation.route).replace(/^app:/, '')
}

/**
 * The cab page's own frame clock, for the host's performance line (crates/host performance.rs): its animation
 * frames counted while the game runs, those that came late (the page waiting for the processor or the graphics
 * card), and the screen shown each second; taken by the host with each line (`westplayReport()`, which starts the
 * counts again). Beside the frames Windows captured of the page, it says which is slow when the screen is: the page
 * itself, or Windows' capture of it (players' screens at a few frames a second, 2026-10-27); and which screen was
 * slow (a player's Maps, 2026-11-09).
 */
export function meterFrames(): () => void {
  let frames = 0
  let slow = 0
  let screens: Record<string, number> = {}
  // The loop counting now (each start is a new one; an older one stops at its next frame).
  let loop = 0
  let running = false
  let shownEach: ReturnType<typeof setInterval> | undefined
  const follow = (on: boolean) => {
    if (on === running) return
    running = on
    const mine = ++loop
    clearInterval(shownEach)
    if (!on) return
    let last: number | null = null
    const tick = (now: number) => {
      if (mine !== loop) return
      frames++
      if (last !== null && now - last >= SLOW_MS) slow++
      last = now
      requestAnimationFrame(tick)
    }
    requestAnimationFrame(tick)
    shownEach = setInterval(() => {
      const name = screenName()
      screens[name] = (screens[name] ?? 0) + 1
    }, 1000)
  }
  const taker = window as { westplayReport?: () => FrameReport }
  taker.westplayReport = () => {
    const report = { frames, slow, screens }
    frames = 0
    slow = 0
    screens = {}
    return report
  }
  follow(useLive.getState().telemetry !== null)
  const stop = useLive.subscribe((live) => follow(live.telemetry !== null))
  return () => {
    stop()
    follow(false)
    clearInterval(shownEach)
    delete taker.westplayReport
  }
}
