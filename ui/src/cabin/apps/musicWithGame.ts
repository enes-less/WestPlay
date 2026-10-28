import { type LiveState, useCabin, useLive } from '../state/store'
import { fadeOutAndPause, playAndFadeIn } from './fade'
import { poweredOff, withGame } from './youtube/withGame'

/**
 * A music app with the game (YouTube, Spotify, Apple Music, YouTube Music; the rule: youtube/withGame.ts):
 * the music playing here fades out and pauses while the player is away from the game (it's paused, or its
 * window minimised) and plays on fading in as they come back, only if it was WestPlay that paused it; it stops at once
 * as the truck's electrics go off, and nothing plays while they're off. Music playing elsewhere (Spotify on a
 * phone) is left as it is.
 */
export interface MusicPlayer {
  /** Its name: one fade at a time for each (fade.ts). */
  name: string
  /** Whether music plays here now. */
  playingHere(): boolean
  pauseHere(): void
  resumeHere(): void
  /** The volume it plays at here (0 to 1), as the player chose it; `null` if there's none to set. */
  volume(): number | null
  /** Sets the volume it plays at (0 to 1) without making it the player's choice: for a fade. */
  sendVolume(share: number): void
  /** Sets the volume it plays at (0 to 1) as the player's choice, as its own slider does: a keybind. */
  setVolume(share: number): void
  /** Called whenever the app's music changes: `started` when it just began playing here. */
  subscribe(changed: (started: boolean) => void): () => void
}

/** Away: the game's window not showing (minimised; in front or not: owner 2026-10-28, as Lossless Scaling's
 * window is in front of the game it scales), or the game paused with its menu up (its menu gone, it's about to
 * run: back, owner 2026-10-21; the game itself says it runs some 1.5 s later). */
function isAway(live: LiveState): boolean {
  return ((live.telemetry?.paused ?? false) && !live.menuGone) || !live.gameShown
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
      fadeOutAndPause(player)
      paused = true
    } else if (action === 'play') {
      playAndFadeIn(player)
    }
    if (!away) paused = false
  })
  const power = useCabin.subscribe((cabin, before) => {
    if (!poweredOff(before.power.phase === 'off', cabin.power.phase === 'off')) return
    // Whatever it's doing, at once (no fade: owner 2026-10-21), and not played on after.
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
