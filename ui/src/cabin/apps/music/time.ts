/** What a song's time is reckoned from (Apple Music's playback, YouTube Music's): playing or not (or waiting for
 * its sound, its time held), seconds into it at `at` (`performance.now()`), and how long it is. */
export interface Timed {
  playing: boolean
  loading: boolean
  position: number
  at: number
  duration: number
}

/** Seconds into the song now (`now`: a clock's last tick, which may be older than the song's own time: never
 * before it, as the shared media clock has it). */
export function positionOf(playback: Timed, now = performance.now()): number {
  const running = playback.playing && !playback.loading
  const position = running ? playback.position + Math.max(0, now - playback.at) / 1000 : playback.position
  return Math.min(position, playback.duration)
}
