import { describe, expect, it, vi } from 'vitest'
import { noteLines } from './update'

// The app's line isn't there in a test (bridge.ts reads the page's address as it loads).
vi.mock('./bridge', () => ({ call: vi.fn(), on: vi.fn() }))

describe('noteLines', () => {
  it('takes the changelog’s list items, without their Markdown', () => {
    const notes = ['- Song lyrics on Now Playing.', '* **Fixed:** the map’s `zoom` on roundabouts.'].join('\n')
    expect(noteLines(notes)).toEqual(['Song lyrics on Now Playing.', 'Fixed: the map’s zoom on roundabouts.'])
  })

  it('leaves out what isn’t a list item: headings, blank lines, text between', () => {
    const notes = [
      '### Added',
      '',
      '- Song lyrics.',
      'Thanks to everyone who tried it.',
      '  - Indented, still one.',
    ].join('\r\n')
    expect(noteLines(notes)).toEqual(['Song lyrics.', 'Indented, still one.'])
  })

  it('joins an item the changelog wraps onto indented lines', () => {
    const notes = [
      '- With map mods, the route follows the mod’s own junctions and the',
      '  Maps app gives its turns there.',
      '- One line.',
    ].join('\r\n')
    expect(noteLines(notes)).toEqual([
      'With map mods, the route follows the mod’s own junctions and the Maps app gives its turns there.',
      'One line.',
    ])
  })

  it('has nothing for notes without a list', () => {
    expect(noteLines('')).toEqual([])
    expect(noteLines('First release.')).toEqual([])
  })
})
