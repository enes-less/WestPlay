import type { MusicPlayer } from './musicWithGame'

/**
 * One player at a time, as on a phone: when one of the cab's players (Apple Music, Spotify, YouTube Music,
 * YouTube) starts playing here, the others playing here pause, and the newest plays on (owner 2026-10-21: a
 * player's report, Spotify and YouTube Music playing over each other). Music playing elsewhere (Spotify on a
 * phone) is left as it is. Returns the function that stops it.
 */
export function oneAtATime(players: readonly MusicPlayer[]): () => void {
  const stops = players.map((player) =>
    player.subscribe((started) => {
      if (!started) return
      for (const other of players) {
        if (other !== player && other.playingHere()) other.pauseHere()
      }
    }),
  )
  return () => {
    for (const stop of stops) stop()
  }
}
