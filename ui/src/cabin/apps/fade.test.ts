import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { fade, levelOf, stopFade } from './fade'

describe('fade', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('steps the volume from one level to another, then goes on', () => {
    const sent: number[] = []
    const done = vi.fn()
    fade('spotify', (share) => sent.push(share), 0.8, 0, 500, done)
    expect(sent[0]).toBe(0.8)
    vi.advanceTimersByTime(250)
    // About half way down.
    expect(sent.at(-1)).toBeGreaterThan(0.3)
    expect(sent.at(-1)).toBeLessThan(0.5)
    expect(done).not.toHaveBeenCalled()
    vi.advanceTimersByTime(300)
    expect(sent.at(-1)).toBe(0)
    expect(done).toHaveBeenCalledOnce()
    // And no more.
    const count = sent.length
    vi.advanceTimersByTime(200)
    expect(sent.length).toBe(count)
  })

  it('a new fade for the same player takes over, the last one left unfinished', () => {
    const sent: number[] = []
    const out = vi.fn()
    fade('spotify', (share) => sent.push(share), 0.8, 0, 500, out)
    vi.advanceTimersByTime(200)
    const back = vi.fn()
    // Where the first left it, for the next to start from.
    const level = levelOf('spotify') ?? 0
    expect(level).toBeGreaterThan(0.4)
    expect(level).toBeLessThan(0.6)
    fade('spotify', (share) => sent.push(share), level, 0.8, 300, back)
    vi.advanceTimersByTime(400)
    expect(out).not.toHaveBeenCalled()
    expect(back).toHaveBeenCalledOnce()
    expect(sent.at(-1)).toBe(0.8)
    // Ended where it should: no level left over.
    expect(levelOf('spotify')).toBeUndefined()
  })

  it("one player's fade leaves another's alone, and can be stopped", () => {
    const a = vi.fn()
    const b = vi.fn()
    fade('spotify', () => {}, 1, 0, 100, a)
    fade('youtube', () => {}, 1, 0, 100, b)
    stopFade('youtube')
    vi.advanceTimersByTime(200)
    expect(a).toHaveBeenCalledOnce()
    expect(b).not.toHaveBeenCalled()
  })
})
