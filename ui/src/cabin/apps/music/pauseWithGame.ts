import { type MusicPlayer, pauseMusicWithGame } from '../musicWithGame'
import { appleMusic, pauseNow, playingNow, resumeNow, setVolume, useAppleMusic } from './appleMusic'

/** Apple Music as one of the cab's players (musicWithGame.ts, oneAtATime.ts). */
export const appleMusicPlayer: MusicPlayer = {
  name: 'apple music',
  playingHere: playingNow,
  pauseHere: pauseNow,
  resumeHere: resumeNow,
  volume: () => useAppleMusic.getState().playback?.volume ?? null,
  sendVolume: (share) => appleMusic({ action: 'volume', percent: Math.round(share * 100) }),
  setVolume: (share) => setVolume(share, true),
  // Started: playing, its sound come (asked to play, it shows playing at once, loading until Apple Music
  // confirms: the fade in waits for that).
  subscribe: (changed) =>
    useAppleMusic.subscribe((music, before) => {
      const sounding = (playback: typeof music.playback) => Boolean(playback?.playing && !playback.loading)
      changed(sounding(music.playback) && !sounding(before.playback))
    }),
}

/** Apple Music with the game (musicWithGame.ts). Returns the function that stops it. */
export function pauseAppleMusicWithGame(): () => void {
  return pauseMusicWithGame(appleMusicPlayer)
}
