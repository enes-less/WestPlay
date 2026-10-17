import { describe, expect, it } from 'vitest'
import { readKept } from './kept'

const item = {
  uri: 'spotify:playlist:37i9dQZF1DX0XUsuxWHRQd',
  kind: 'playlist',
  name: 'RapCaviar',
  by: 'Spotify',
  description: '',
  image: 'https://i.scdn.co/image/ab67706f00000002',
  color: '#3a2a1a',
}

describe('readKept', () => {
  it('reads Home and the Library back as they were kept', () => {
    const kept = readKept(
      JSON.stringify({ shortcuts: [item], recent: { title: 'Recently played', items: [item] }, library: [item] }),
    )
    expect(kept.shortcuts).toHaveLength(1)
    expect(kept.recent?.title).toBe('Recently played')
    expect(kept.library?.[0].image).toBe('https://i.scdn.co/image/ab67706f00000002')
  })

  it('keeps a picture only from Spotify image hosts, and only items that are items', () => {
    const kept = readKept(
      JSON.stringify({
        shortcuts: [{ ...item, image: 'https://evil.example/x.png' }, { ...item, uri: 'javascript:x' }, { name: 'x' }],
        library: null,
      }),
    )
    expect(kept.shortcuts).toHaveLength(1)
    expect(kept.shortcuts[0].image).toBeNull()
    expect(kept.library).toBeNull()
  })

  it('keeps nothing of anything else', () => {
    expect(readKept('not json').shortcuts).toEqual([])
    expect(readKept(null).recent).toBeNull()
  })
})
