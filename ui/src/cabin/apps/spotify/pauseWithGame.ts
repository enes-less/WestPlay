import { type MusicPlayer, pauseMusicWithGame } from '../musicWithGame'
import { pauseHere, playingHere, resumeHere, setVolume, spotify, useSpotify } from './spotify'

/** Spotify as one of the cab's players (musicWithGame.ts, oneAtATime.ts): only music playing here, not on
 * another of the account's devices (a phone, through Spotify Connect). */
export const spotifyPlayer: MusicPlayer = {
  name: 'spotify',
  playingHere,
  pauseHere,
  resumeHere,
  // The volume of the device it plays on here (`null`: on another device, or none Spotify can set).
  volume: () => {
    const playback = useSpotify.getState().playback
    return playback && playback.here !== false ? playback.volume : null
  },
  sendVolume: (share) => spotify({ action: 'volume', percent: Math.round(share * 100) }),
  setVolume: (share) => setVolume(share, true),
  subscribe: (changed) =>
    useSpotify.subscribe((spotify, before) => {
      const now = spotify.playback
      changed(Boolean(now?.playing && now.here && !before.playback?.playing))
    }),
}

/** Spotify with the game (musicWithGame.ts). Returns the function that stops it. */
export function pauseSpotifyWithGame(): () => void {
  return pauseMusicWithGame(spotifyPlayer)
}
