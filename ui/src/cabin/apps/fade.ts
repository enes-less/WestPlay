import type { MusicPlayer } from './musicWithGame'

/**
 * A player's volume faded, as the music pauses and plays on with the game (musicWithGame.ts; owner
 * 2026-10-21: 0.5 s out leaving the game, 0.3 s in coming back). The player's own volume is stepped by `send`
 * (its raw command: the volume the player chose stays as it was, and so does its slider); one fade at a time
 * for each player, a new one taking over from the last, and from a fade in still waiting for the player to
 * play (`playAndFadeIn`).
 */

/** How often the volume is stepped, ms. */
const STEP_MS = 30

/** Each player's fade running, to stop as another starts. */
const running = new Map<string, () => void>()
/** Each player's level as a fade last sent it (gone once a fade ends where it should). */
const levels = new Map<string, number>()
/** Each player's fade in waiting for it to play again (`playAndFadeIn`), to call off as anything else comes. */
const waiting = new Map<string, () => void>()

/** The level `key`'s player was last faded to, if a fade left it somewhere (one stopped half way). */
export function levelOf(key: string): number | undefined {
  return levels.get(key)
}

/**
 * Fades `key`'s player from `from` to `to` (0 to 1) over `ms`, by `send`, then calls `done`. A fade still
 * running for it stops where it is (its `done` not called).
 */
export function fade(
  key: string,
  send: (share: number) => void,
  from: number,
  to: number,
  ms: number,
  done?: () => void,
): void {
  running.get(key)?.()
  waiting.get(key)?.()
  const started = performance.now()
  const sendAt = (share: number) => {
    levels.set(key, share)
    send(share)
  }
  const step = () => {
    const t = Math.min(1, (performance.now() - started) / ms)
    sendAt(from + (to - from) * t)
    if (t < 1) return
    stop()
    levels.delete(key)
    done?.()
  }
  const timer = setInterval(step, STEP_MS)
  const stop = () => {
    clearInterval(timer)
    if (running.get(key) === stop) running.delete(key)
  }
  running.set(key, stop)
  sendAt(from)
}

/** Stops `key`'s fade, or its fade in waiting for it to play, if one (its `done` not called); the level it
 * left is forgotten (whatever's sent next is the player's volume). */
export function stopFade(key: string): void {
  running.get(key)?.()
  waiting.get(key)?.()
  levels.delete(key)
}

/** How long the music fades out as the player leaves the game, and in as they come back, ms (owner
 * 2026-10-21: coming back, the game holds the truck about that long as it unpauses). */
const FADE_OUT_MS = 500
const FADE_IN_MS = 300
/** How long a player is given to play again before its volume is simply set back, ms. */
const PLAY_WAIT_MS = 2000

/** Fades `player` out, pauses it, and sets its volume back (unheard): the player leaving the game. With no
 * volume to fade, it just pauses. */
export function fadeOutAndPause(player: MusicPlayer): void {
  const volume = player.volume()
  if (volume === null || volume <= 0) {
    player.pauseHere()
    return
  }
  const from = levelOf(player.name) ?? volume
  if (from <= 0) {
    // Silent already (back a moment ago, its fade in still waiting for it to play): paused at once.
    stopFade(player.name)
    player.pauseHere()
    player.sendVolume(volume)
    return
  }
  fade(player.name, player.sendVolume, from, 0, FADE_OUT_MS, () => {
    player.pauseHere()
    player.sendVolume(volume)
  })
}

/** Plays `player` on and fades it in once it plays (it takes a moment to start again): the player back in the
 * game. Still playing (back before the fade out ended): faded back up from where it got to. */
export function playAndFadeIn(player: MusicPlayer): void {
  const volume = player.volume()
  if (volume === null || volume <= 0) {
    stopFade(player.name)
    player.resumeHere()
    return
  }
  if (player.playingHere()) {
    fade(player.name, player.sendVolume, levelOf(player.name) ?? volume, volume, FADE_IN_MS)
    return
  }
  stopFade(player.name)
  levels.set(player.name, 0)
  player.sendVolume(0)
  // Waiting for it to play, until it does or anything else comes for it (a fade, a stop: the player away
  // again before it sounded, owner's keys): called off then, a late fade in would undo it.
  let over = false
  const callOff = () => {
    if (over) return
    over = true
    stopWaiting()
    clearTimeout(timer)
    if (waiting.get(player.name) === callOff) waiting.delete(player.name)
  }
  const begin = () => {
    if (over) return
    callOff()
    fade(player.name, player.sendVolume, 0, volume, FADE_IN_MS)
  }
  // Listening before it's asked to play: a player that shows itself playing the moment it's asked (Spotify,
  // YouTube Music) says so in that call, and missed, the fade waited out PLAY_WAIT_MS in silence (owner
  // 2026-10-21: the music came back late).
  const stopWaiting = player.subscribe((started) => {
    if (started) begin()
  })
  const timer = setTimeout(begin, PLAY_WAIT_MS)
  waiting.set(player.name, callOff)
  player.resumeHere()
}
