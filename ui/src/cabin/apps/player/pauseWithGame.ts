import { type MusicPlayer, pauseMusicWithGame } from '../musicWithGame'
import { pauseNow, playingNow, resumeNow, sendVolume, setVolume, skip, usePlayer } from './player'

/** Player as one of the cab's players (musicWithGame.ts, oneAtATime.ts, keybinds.ts). */
export const playerAppPlayer: MusicPlayer = {
  name: 'player',
  playingHere: playingNow,
  pauseHere: pauseNow,
  resumeHere: resumeNow,
  volume: () => usePlayer.getState().volume,
  sendVolume,
  setVolume,
  skip,
  // Started: playing, its sound come (the fade in waits for that).
  subscribe: (changed) =>
    usePlayer.subscribe((state, before) => {
      const sounding = (playback: typeof state.playback) => Boolean(playback?.playing && !playback.loading)
      changed(sounding(state.playback) && !sounding(before.playback))
    }),
}

/** Player with the game (musicWithGame.ts). Returns the function that stops it. */
export function pausePlayerWithGame(): () => void {
  return pauseMusicWithGame(playerAppPlayer)
}
