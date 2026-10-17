import type { Lyrics } from '../../shared/host'

/** How far ahead of its time a line lights (the eye reads a moment before the voice sings it). */
const LEAD = 0.25

/** The line sung at `position` (seconds into the song): the last one started, -1 before the first. */
export function lineAt(lines: Lyrics['lines'], position: number): number {
  let low = 0
  let high = lines.length - 1
  let found = -1
  while (low <= high) {
    const middle = (low + high) >> 1
    if (lines[middle].start <= position + LEAD) {
      found = middle
      low = middle + 1
    } else {
      high = middle - 1
    }
  }
  return found
}

/** The preview's words (WestPlay's own, of the road), a line every few seconds from the song's start, in stanzas
 * of four: as a song's synced words come. */
const PREVIEW_STANZAS = [
  [
    'Headlights on the long road home',
    'Mile by mile the night unfolds',
    'Radio low, the engine hums',
    'Somewhere east the morning comes',
  ],
  [
    'Border signs in another tongue',
    'Coffee cold, the tank half full',
    'Every town a name unsung',
    'Every bridge another pull',
  ],
  [
    'Keep it steady, keep it slow',
    'There’s a dock where the lights still glow',
    'Keep it steady, let it roll',
    'Every road leads someone home',
  ],
]

export function previewLyrics(colors?: Lyrics['colors'], provider?: string): Lyrics {
  const lines: Lyrics['lines'] = []
  for (let at = 8, n = 0; at < 300; n++) {
    const stanza = PREVIEW_STANZAS[Math.floor(n / 4) % PREVIEW_STANZAS.length]
    lines.push({ start: at, end: at + 4, text: stanza[n % 4], ...(n > 0 && n % 4 === 0 ? { stanza: true } : {}) })
    at += n % 4 === 3 ? 8 : 4.5
  }
  return {
    synced: true,
    lines,
    writers: ['WestPlay'],
    ...(colors ? { colors } : {}),
    ...(provider ? { provider } : {}),
  }
}
