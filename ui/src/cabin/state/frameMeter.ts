import { useLive } from './store'

/**
 * The cab page's own frame clock, for the host's performance line (crates/host performance.rs): its animation
 * frames counted while the game runs, taken by the host with each line (`westplayFrames()`, which starts the
 * count again). Beside the frames Windows captured of the page, it says which is slow when the screen is: the
 * page itself (Chromium given too little of the processor or the graphics card), or Windows' capture of it
 * (players' screens at a few frames a second, 2026-10-27).
 */
export function meterFrames(): () => void {
  let frames = 0
  // The loop counting now (each start is a new one; an older one stops at its next frame).
  let loop = 0
  let running = false
  const follow = (on: boolean) => {
    if (on === running) return
    running = on
    const mine = ++loop
    if (!on) return
    const tick = () => {
      if (mine !== loop) return
      frames++
      requestAnimationFrame(tick)
    }
    requestAnimationFrame(tick)
  }
  const taker = window as { westplayFrames?: () => number }
  taker.westplayFrames = () => {
    const counted = frames
    frames = 0
    return counted
  }
  follow(useLive.getState().telemetry !== null)
  const stop = useLive.subscribe((live) => follow(live.telemetry !== null))
  return () => {
    stop()
    follow(false)
    delete taker.westplayFrames
  }
}
