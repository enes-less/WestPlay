import type { KeybindAction } from '../../shared/host'
import { levelOf, stopFade } from './fade'
import type { MusicPlayer } from './musicWithGame'

/**
 * The player's keybinds pressed in the game (the companion's Controls; the host's `keybind` message): the
 * music's volume a step up or down, or it paused or played on. It's the player playing here, else the one
 * that last played here; with none, nothing happens.
 */

/** A volume step (owner 2026-10-21: 5%). */
export const VOLUME_STEP = 0.05

/** What a keybind did, to show: the volume it set (0 to 1). */
export interface VolumeShown {
  volume: number
}

/** The keybinds for `players` (the cab's music apps). `press` acts on one; `stop` stops following them. */
export function keybinds(players: readonly MusicPlayer[]): {
  press(action: KeybindAction): VolumeShown | null
  stop(): void
} {
  let last: MusicPlayer | null = null
  const stops = players.map((player) =>
    player.subscribe((started) => {
      if (started) last = player
    }),
  )
  const current = () => players.find((player) => player.playingHere()) ?? last
  return {
    press(action) {
      const player = current()
      if (!player) return null
      if (action === 'playPause') {
        // A fade with the game stops where it is; the volume the player chose comes back with it.
        const left = levelOf(player.name) !== undefined
        stopFade(player.name)
        const volume = player.volume()
        if (left && volume !== null) player.sendVolume(volume)
        if (player.playingHere()) player.pauseHere()
        else player.resumeHere()
        return null
      }
      const volume = player.volume()
      if (volume === null) return null
      stopFade(player.name)
      const next = stepped(volume, action === 'volumeUp' ? 1 : -1)
      player.setVolume(next)
      return { volume: next }
    },
    stop() {
      for (const stop of stops) stop()
    },
  }
}

/** `volume` a step up (1) or down (-1), onto the steps (a volume between two goes to the next one). */
export function stepped(volume: number, direction: 1 | -1): number {
  const steps = 1 / VOLUME_STEP
  // Close enough to a step counts as on it (a volume the player set as 0.6 may come back as 0.6000001).
  const at = volume * steps
  const on = Math.abs(at - Math.round(at)) < 0.01 ? Math.round(at) : null
  const next = on !== null ? on + direction : direction > 0 ? Math.ceil(at) : Math.floor(at)
  return Math.min(steps, Math.max(0, next)) / steps
}
