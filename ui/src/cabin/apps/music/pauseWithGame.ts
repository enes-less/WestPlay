import { pauseMusicWithGame } from '../musicWithGame'
import { pauseNow, playingNow, resumeNow, useAppleMusic } from './appleMusic'

/** Apple Music with the game (musicWithGame.ts). Returns the function that stops it. */
export function pauseAppleMusicWithGame(): () => void {
  return pauseMusicWithGame({
    playingHere: playingNow,
    pauseHere: pauseNow,
    resumeHere: resumeNow,
    subscribe: (changed) =>
      useAppleMusic.subscribe((music, before) => {
        changed(Boolean(music.playback?.playing && !before.playback?.playing))
      }),
  })
}
