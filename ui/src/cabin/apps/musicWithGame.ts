import { type LiveState, useCabin, useLive } from '../state/store'
import { poweredOff, withGame } from './youtube/withGame'

/**
 * A music app with the game, as YouTube is (youtube/pauseWithGame.ts, the same rule: youtube/withGame.ts):
 * the music playing here pauses while the player is away from the game (it's paused, or tabbed out of) and
 * plays on as they come back, only if it was WestPlay that paused it; it stops as the truck's electrics go
 * off, and nothing plays while they're off. Music playing elsewhere (Spotify on a phone) is left as it is.
 */
export interface MusicPlayer {
  /** Whether music plays here now. */
  playingHere(): boolean
  pauseHere(): void
  resumeHere(): void
  /** Called whenever the app's music changes: `started` when it just began playing here. */
  subscribe(changed: (started: boolean) => void): () => void
}

function isAway(live: LiveState): boolean {
  return (live.telemetry?.paused ?? false) || !live.gameFocused
}

/** The truck's electrics are off (as seen: not before the first telemetry says so). */
function isOff(): boolean {
  const { power } = useCabin.getState()
  return power.observed && power.phase === 'off'
}

/** Pauses `player` with the game, and stops it as the truck's electrics go off, from now on. Returns the
 * function that stops it. */
export function pauseMusicWithGame(player: MusicPlayer): () => void {
  let paused = false
  const game = useLive.subscribe((live, before) => {
    const away = isAway(live)
    const wasAway = isAway(before)
    if (away === wasAway) return
    const action = withGame(away, wasAway, player.playingHere(), paused, !isOff())
    if (action === 'pause') {
      player.pauseHere()
      paused = true
    } else if (action === 'play') {
      player.resumeHere()
    }
    if (!away) paused = false
  })
  const power = useCabin.subscribe((cabin, before) => {
    if (!poweredOff(before.power.phase === 'off', cabin.power.phase === 'off')) return
    // Whatever it's doing, and not played on after.
    if (player.playingHere()) player.pauseHere()
    paused = false
  })
  // Nothing plays while they're off: music that starts playing here then is paused as it starts.
  const dark = player.subscribe((started) => {
    if (isOff() && started) player.pauseHere()
  })
  return () => {
    game()
    power()
    dark()
  }
}
