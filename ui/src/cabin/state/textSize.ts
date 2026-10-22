/**
 * The text sizes the player chooses from (Settings › Appearance › Text Size), as iOS's seven steps: its body
 * text 14 to 23 pt about the default 17's, each a scale of the apps' text (their `font-size`s times
 * `--text-scale`). Only the text: icons, artwork, keys and spacing stay as they are (owner 2026-10-22).
 */
export const TEXT_SIZES = [0.82, 0.88, 0.94, 1, 1.12, 1.24, 1.35] as const

/** The step WestPlay starts on: iOS's own default, the apps' text as designed. */
export const DEFAULT_TEXT_SIZE = 3

/** A step kept or sent, as one there is (anything else: the default). */
export function toTextSize(value: unknown): number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0 && value < TEXT_SIZES.length
    ? value
    : DEFAULT_TEXT_SIZE
}

/** How much a step scales the apps' text. */
export function textScale(step: number): number {
  return TEXT_SIZES[toTextSize(step)]
}
