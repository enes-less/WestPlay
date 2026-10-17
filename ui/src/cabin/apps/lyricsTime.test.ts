import { describe, expect, it } from 'vitest'
import { lineAt, previewLyrics } from './lyricsTime'

describe('the line sung', () => {
  const lines = [10, 14, 20, 31].map((start) => ({ start, end: 0, text: String(start) }))

  it('is none before the first, then the last one started (a moment early)', () => {
    expect(lineAt(lines, 0)).toBe(-1)
    expect(lineAt(lines, 9.8)).toBe(0)
    expect(lineAt(lines, 14)).toBe(1)
    expect(lineAt(lines, 19.5)).toBe(1)
    expect(lineAt(lines, 400)).toBe(3)
    expect(lineAt([], 5)).toBe(-1)
  })

  it('in the preview, comes in order through the song', () => {
    const preview = previewLyrics().lines
    expect(preview.every((line, i) => i === 0 || line.start > preview[i - 1].start)).toBe(true)
    expect(preview.filter((line) => line.stanza).length).toBeGreaterThan(2)
  })
})
