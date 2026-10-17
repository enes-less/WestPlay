import { type LiveState, useCabin, useLive } from '../../state/store'
import { poweredOff, withGame } from './withGame'
import { command, useYouTube } from './youtube'

function isAway(live: LiveState): boolean {
  return (live.telemetry?.paused ?? false) || !live.gameFocused
}

/** The truck's electrics are off (as seen: not before the first telemetry says so). */
function isOff(): boolean {
  const { power } = useCabin.getState()
  return power.observed && power.phase === 'off'
}

/** Pauses YouTube with the game, and stops it as the truck's electrics go off, from now on. Returns the
 * function that stops it. */
export function pauseWithGame(): () => void {
  let paused = false
  const game = useLive.subscribe((live, before) => {
    const away = isAway(live)
    const wasAway = isAway(before)
    if (away === wasAway) return
    const state = useYouTube.getState().state
    // Playing, or about to (buffering).
    const playing = state !== null && (state.state === 1 || state.state === 3)
    const action = withGame(away, wasAway, playing, paused, !isOff())
    if (action === 'pause') {
      command({ action: 'pause' })
      paused = true
    } else if (action === 'play') {
      command({ action: 'play' })
    }
    if (!away) paused = false
  })
  const power = useCabin.subscribe((cabin, before) => {
    if (!poweredOff(before.power.phase === 'off', cabin.power.phase === 'off')) return
    // Whatever it's doing (a video still starting too), and not played on after.
    if (useYouTube.getState().state) command({ action: 'pause' })
    paused = false
  })
  // Nothing plays while they're off: a video that was still starting as they went off is paused as it starts.
  const dark = useYouTube.subscribe((youtube, before) => {
    if (isOff() && youtube.state?.state === 1 && before.state?.state !== 1) command({ action: 'pause' })
  })
  return () => {
    game()
    power()
    dark()
  }
}
