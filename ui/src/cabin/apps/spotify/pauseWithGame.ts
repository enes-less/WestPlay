import { type MusicPlayer, pauseMusicWithGame } from '../musicWithGame'
import { pauseHere, playingHere, resumeHere, setVolume, spotify, useSpotify } from './spotify'

/** The volume of the device it plays on here (`null`: on another device, or none Spotify can set). */
function volume(): number | null {
  const playback = useSpotify.getState().playback
  return playback && playback.here !== false ? playback.volume : null
}

/** Spotify as one of the cab's players (musicWithGame.ts, oneAtATime.ts): only music playing here, not on
 * another of the account's devices (a phone, through Spotify Connect). Whether it plays, and when it starts,
 * as its player in its page says at once; a fade in the page's own player, Spotify's volume (a round trip to
 * its servers each step) left as chosen (owner 2026-10-24: "Song starts too late, and then alt tabbing is
 * totally fucked"). */
export const spotifyPlayer: MusicPlayer = {
  name: 'spotify',
  playingHere,
  pauseHere,
  resumeHere,
  volume,
  sendVolume: (share) => {
    const chosen = volume()
    spotify({ action: 'fade', level: chosen ? Math.min(1, share / chosen) : 1 })
  },
  setVolume: (share) => setVolume(share, true),
  subscribe: (changed) =>
    useSpotify.subscribe((now, before) =>
      changed(Boolean(now.playing && !before.playing && now.playback?.here !== false)),
    ),
}

/** Spotify with the game (musicWithGame.ts). Returns the function that stops it. */
export function pauseSpotifyWithGame(): () => void {
  return pauseMusicWithGame(spotifyPlayer)
}
