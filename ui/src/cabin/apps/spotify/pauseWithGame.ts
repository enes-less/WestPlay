import { pauseMusicWithGame } from '../musicWithGame'
import { pauseHere, playingHere, resumeHere, useSpotify } from './spotify'

/** Spotify with the game (musicWithGame.ts): only music playing here, not on another of the account's devices
 * (a phone, through Spotify Connect). Returns the function that stops it. */
export function pauseSpotifyWithGame(): () => void {
  return pauseMusicWithGame({
    playingHere,
    pauseHere,
    resumeHere,
    subscribe: (changed) =>
      useSpotify.subscribe((spotify, before) => {
        const now = spotify.playback
        changed(Boolean(now?.playing && now.here && !before.playback?.playing))
      }),
  })
}
