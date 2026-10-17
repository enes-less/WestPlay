import { describe, expect, it } from 'vitest'
import { formatClock } from './time'

describe('formatClock', () => {
  it('writes times as YouTube does', () => {
    expect(formatClock(0)).toBe('0:00')
    expect(formatClock(41.9)).toBe('0:41')
    expect(formatClock(814)).toBe('13:34')
    expect(formatClock(3730)).toBe('1:02:10')
  })

  it('takes nonsense as zero', () => {
    expect(formatClock(-5)).toBe('0:00')
    expect(formatClock(Number.NaN)).toBe('0:00')
    expect(formatClock(Infinity)).toBe('0:00')
  })
})
