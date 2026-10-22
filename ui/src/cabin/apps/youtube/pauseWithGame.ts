import type { YouTubeState } from '../../../shared/host'
import { type MusicPlayer, pauseMusicWithGame } from '../musicWithGame'
import { command, setVolume, useYouTube } from './youtube'

/** A video playing, or about to (buffering). */
function isPlaying(state: YouTubeState | null): boolean {
  return state !== null && (state.state === 1 || state.state === 3)
}

/** YouTube as one of the cab's players (oneAtATime.ts). */
export const youtubePlayer: MusicPlayer = {
  name: 'youtube',
  playingHere: () => isPlaying(useYouTube.getState().state),
  pauseHere: () => command({ action: 'pause' }),
  resumeHere: () => command({ action: 'play' }),
  // The volume every video plays at (none while muted).
  volume: () => {
    const { volume, muted } = useYouTube.getState()
    return muted ? 0 : volume / 100
  },
  sendVolume: (share) => command({ action: 'volume', percent: Math.round(share * 100) }),
  setVolume: (share) => setVolume(share * 100),
  subscribe: (changed) =>
    useYouTube.subscribe((youtube, before) => changed(isPlaying(youtube.state) && !isPlaying(before.state))),
}

/** YouTube with the game (musicWithGame.ts): a video still starting as the truck's electrics go off is paused
 * as it starts (buffering). Returns the function that stops it. */
export function pauseYouTubeWithGame(): () => void {
  return pauseMusicWithGame(youtubePlayer)
}
