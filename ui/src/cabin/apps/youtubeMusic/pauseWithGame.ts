import { pauseMusicWithGame } from '../musicWithGame'
import { pauseNow, playingNow, resumeNow, useYouTubeMusic } from './youtubeMusic'

/** YouTube Music with the game (musicWithGame.ts). Returns the function that stops it. */
export function pauseYouTubeMusicWithGame(): () => void {
  return pauseMusicWithGame({
    playingHere: playingNow,
    pauseHere: pauseNow,
    resumeHere: resumeNow,
    subscribe: (changed) =>
      useYouTubeMusic.subscribe((music, before) => {
        changed(Boolean(music.playback?.playing && !before.playback?.playing))
      }),
  })
}
