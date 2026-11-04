import { describe, expect, it } from 'vitest'
import { cursorArea } from './cursorArea'

describe("the host's cursor's area", () => {
  const frame = { width: 1280, height: 720 }

  it('is the CarPlay area under its black corners, never on the frame’s outermost pixels', () => {
    // The whole screen's CarPlay: two pixels in from every edge, its corners as round less them.
    expect(cursorArea({ left: 0, top: 0, right: 1280, bottom: 720, radius: 78.7 }, frame, false)).toEqual({
      left: 2,
      top: 2,
      right: 1278,
      bottom: 718,
      radius: 76.7,
    })
    // Beside a truck's strip (the Smart Dash's, left and below): the area as it is, inside the frame.
    expect(cursorArea({ left: 136, top: 0, right: 1280, bottom: 640, radius: 70 }, frame, false)).toEqual({
      left: 136,
      top: 2,
      right: 1278,
      bottom: 640,
      radius: 68,
    })
  })

  it('is the whole frame while the screen is handed to the truck (WestPlay’s button over it)', () => {
    expect(cursorArea({ left: 600, top: 300, right: 700, bottom: 350, radius: 6 }, frame, true)).toEqual({
      left: 2,
      top: 2,
      right: 1278,
      bottom: 718,
      radius: 0,
    })
  })
})
