/**
 * How often the map is drawn: on every display frame, or every 2nd, 3rd... on a fast display, evenly, and never
 * fewer than [`LEAST_PACE`] a second. The host hands the game at most 60 of the page's frames a second, the newest
 * at each 60th of a second on its own clock (crates/host capture.rs `MAX_FPS`): only a map drawn evenly and more
 * often than that has a new frame in every one it hands over. Capped at 60 itself (a frame a 60th of a second on
 * from the last, 4 of 5 at 75 Hz), the map's frames and the host's fell on different display frames, and the game
 * got as few as 45 new map frames a second at 75 Hz, 36 at 144 Hz, held up to 40 ms (owner 2026-11-12: "moving like
 * steps", even on an RTX 4080; the map drew each in 1-2 ms). Measured by simulating the two clocks: drawn evenly at
 * 70 a second or more, 60 new frames reach the game at every display rate, its frames' timing wavering by 2 ms;
 * drawn evenly at exactly 60 (every 2nd at 120 Hz), as few as 47.
 */

/** The fewest the map is drawn a second when it isn't drawn on every display frame. */
export const LEAST_PACE = 70
/** How many display frames' lengths the display's is measured over (the mean of their middle half: a frame the page
 * was late for doesn't move it, nor frames whose timing wavers either way). */
const MEASURED = 32

/** The display frames from one the map is drawn on to the next, at a display frame of `frameMs`. */
export function framesApart(frameMs: number): number {
  return Math.max(1, Math.floor(1000 / frameMs / LEAST_PACE))
}

/** The display's frame length, ms, from the times between the last display frames (`gaps`, newest last); a 60th
 * of a second until there are any. */
export function displayFrameMs(gaps: readonly number[]): number {
  if (gaps.length === 0) return 1000 / 60
  const sorted = [...gaps].sort((a, b) => a - b)
  const middle = sorted.slice(Math.floor(sorted.length / 4), Math.max(Math.ceil((sorted.length * 3) / 4), 1))
  return middle.reduce((sum, gap) => sum + gap, 0) / middle.length
}

/** Whether the display frame at `now` is one the map is drawn on, the map last drawn at `last` (ms): when
 * [`framesApart`] frames have gone by since (half a frame early is in time: a frame's timing wavers). After a pause,
 * at once. */
export function mapFrameDue(now: number, last: number, frameMs: number): boolean {
  return now - last >= (framesApart(frameMs) - 0.5) * frameMs
}

/** The display's frames as they come: their lengths, kept for [`displayFrameMs`]. */
export function displayClock(): { tick(now: number): number } {
  const gaps: number[] = []
  let previous: number | null = null
  return {
    /** The display frame at `now`: the display's frame length, measured with it. */
    tick(now: number) {
      if (previous !== null && now > previous) {
        gaps.push(now - previous)
        if (gaps.length > MEASURED) gaps.shift()
      }
      previous = now
      return displayFrameMs(gaps)
    },
  }
}
