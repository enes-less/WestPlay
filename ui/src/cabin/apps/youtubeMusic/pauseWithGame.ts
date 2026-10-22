import { type MusicPlayer, pauseMusicWithGame } from '../musicWithGame'
import { pauseNow, playingNow, resumeNow, setVolume, useYouTubeMusic, youtubeMusic } from './youtubeMusic'

/** YouTube Music as one of the cab's players (musicWithGame.ts, oneAtATime.ts). */
export const youtubeMusicPlayer: MusicPlayer = {
  name: 'youtube music',
  playingHere: playingNow,
  pauseHere: pauseNow,
  resumeHere: resumeNow,
  volume: () => useYouTubeMusic.getState().playback?.volume ?? null,
  sendVolume: (share) => youtubeMusic({ action: 'volume', percent: Math.round(share * 100) }),
  setVolume: (share) => setVolume(share, true),
  subscribe: (changed) =>
    useYouTubeMusic.subscribe((music, before) => {
      changed(Boolean(music.playback?.playing && !before.playback?.playing))
    }),
}

/** YouTube Music with the game (musicWithGame.ts). Returns the function that stops it. */
export function pauseYouTubeMusicWithGame(): () => void {
  return pauseMusicWithGame(youtubeMusicPlayer)
}
