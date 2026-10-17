/** A place name's box on screen: its centre and size, pixels. */
export interface LabelBox {
  x: number
  y: number
  width: number
  height: number
}

/**
 * Which names to show, so none overlap: taken in order of importance (the biggest place first), each kept
 * if its box, grown by `padding`, touches none already kept. `boxes` are in that order; `null` for a name
 * not in view. Returns the indices kept.
 */
export function placeLabels(boxes: readonly (LabelBox | null)[], padding: number): Set<number> {
  const kept: LabelBox[] = []
  const shown = new Set<number>()
  boxes.forEach((box, index) => {
    if (!box) return
    const overlaps = kept.some(
      (other) =>
        Math.abs(box.x - other.x) * 2 < box.width + other.width + padding * 2 &&
        Math.abs(box.y - other.y) * 2 < box.height + other.height + padding * 2,
    )
    if (!overlaps) {
      kept.push(box)
      shown.add(index)
    }
  })
  return shown
}

/** Writing systems a place's name can be in, to tell when two names read differently. */
const SCRIPTS: [name: string, letter: RegExp][] = [
  ['latin', /\p{Script=Latin}/u],
  ['greek', /\p{Script=Greek}/u],
  ['cyrillic', /\p{Script=Cyrillic}/u],
  ['georgian', /\p{Script=Georgian}/u],
  ['cjk', /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u],
]

/** The script a name is written in: its first letter's (`other` for one not listed, `none` without letters). */
export function scriptOf(text: string): string {
  for (const char of text) {
    const script = SCRIPTS.find(([, letter]) => letter.test(char))
    if (script) return script[0]
    if (/\p{L}/u.test(char)) return 'other'
  }
  return 'none'
}

/**
 * What a place's label says: its name in the player's language (`localized`, when that differs from its
 * own name). But where its own name is in another script (София, for a Turkish player), that goes on top,
 * as the signs there have it, with the player's name for it under it.
 */
export function labelLines(name: string, localized?: string): { first: string; second: string | null } {
  if (!localized) return { first: name, second: null }
  if (scriptOf(name) !== scriptOf(localized)) return { first: name, second: localized }
  return { first: localized, second: null }
}
