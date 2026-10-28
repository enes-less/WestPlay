/** What to do with the video as the player goes away from the game or comes back. */
export type WithGame = 'pause' | 'play' | null

/**
 * The video pauses while the player is away from the game (it's paused: its menu, its help; or its window
 * doesn't show: minimised) and plays on as they come back, but only if it was playing when they left:
 * one the player paused stays paused. `paused`: WestPlay paused it for this. `powered`: the truck's
 * electrics are on (off, nothing plays on: the screen is dark, as CarPlay is gone).
 */
export function withGame(
  away: boolean,
  wasAway: boolean,
  playing: boolean,
  paused: boolean,
  powered: boolean,
): WithGame {
  if (away && !wasAway && playing) return 'pause'
  if (!away && wasAway && paused && powered) return 'play'
  return null
}

/** Whether the truck's electrics just went off: the video stops (paused, and left so: nothing plays it on
 * again by itself, not the game coming back nor the power). */
export function poweredOff(wasOff: boolean, off: boolean): boolean {
  return off && !wasOff
}
