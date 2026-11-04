/** How long the screen goes untouched before the Dashboard's map grows, ms (owner 2026-10-22). */
export const UNTOUCHED_MS = 30_000
/** From how fast (km/h) it grows (owner 2026-11-03: "after we pass 30 km/h"; stopping, no need for it). */
export const GROWS_FROM_KMH = 30

/** Whether the Dashboard's map grows now: `on` (the setting, and widgets to make room from), the truck at `kmh`
 * (either way) of [`GROWS_FROM_KMH`] or more, the screen untouched `untouchedMs` of [`UNTOUCHED_MS`] or more. */
export function mapGrows(on: boolean, kmh: number, untouchedMs: number): boolean {
  return on && Math.abs(kmh) >= GROWS_FROM_KMH && untouchedMs >= UNTOUCHED_MS
}
