/** The most the map is drawn a second, and the time between: the game takes no more (capture.rs's MAX_FPS). */
export const MAP_FPS = 60
const MAP_FRAME_MS = 1000 / MAP_FPS
/** How early a display frame may come and still be the one drawn on (its timing wavers by a millisecond or so,
 * and a 60 Hz display's every frame is drawn), ms. */
const DUE_SLACK_MS = 2

/**
 * Whether the display's frame at `now` is one the map is drawn on, given when the next was `due` (ms): the next
 * one's due time if so, `null` if not. Each due a 60th of a second after the last, so the frames drawn average
 * [`MAP_FPS`] on any display (4 of 5 at 75 Hz, every 2nd or 3rd at 144 Hz, every one at 60 Hz); after a pause
 * (more than a frame late), counted again from now.
 */
export function mapFrameDue(now: number, due: number): number | null {
  if (now < due - DUE_SLACK_MS) return null
  return now - due > MAP_FRAME_MS ? now + MAP_FRAME_MS : due + MAP_FRAME_MS
}
