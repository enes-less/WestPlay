import { type MusicPlayer, pauseMusicWithGame } from '../musicWithGame'
import { send, useSafari } from './safari'

/** The sound's volume the player chose for Safari's pages (0 to 1: the keybinds'), on top of each page's own. */
let chosen = 1

/**
 * Safari as one of the cab's players (musicWithGame.ts, oneAtATime.ts, keybinds.ts): a page's sound pauses with the
 * game and for another player, as the music apps' does (owner 2026-11-07), faded as theirs is. It has no songs to
 * skip.
 */
export const safariPlayer: MusicPlayer = {
  name: 'safari',
  playingHere: () => useSafari.getState().playing,
  pauseHere: () => send({ action: 'pause' }),
  resumeHere: () => send({ action: 'resume' }),
  volume: () => chosen,
  sendVolume: (share) => send({ action: 'level', level: share }),
  setVolume: (share) => {
    chosen = Math.min(1, Math.max(0, share))
    send({ action: 'level', level: chosen })
  },
  skip: () => {},
  subscribe: (changed) => useSafari.subscribe((state, before) => changed(state.playing && !before.playing)),
}

/** Safari with the game (musicWithGame.ts). Returns the function that stops it. */
export function pauseSafariWithGame(): () => void {
  return pauseMusicWithGame(safariPlayer)
}
