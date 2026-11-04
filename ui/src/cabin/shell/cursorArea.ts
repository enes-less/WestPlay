/** How far inside the frame's edge the cursor stays (page pixels): the game repeats the frame's outermost pixels
 * into a screen's rim (the Smart Dash's, `screen-rim` in shell.css: 2 px), and a cursor on them lit its margin
 * (owner 2026-11-03). */
export const CURSOR_INSET = 2

/** A rectangle with round corners, in page pixels. */
export interface RoundedArea {
  left: number
  top: number
  right: number
  bottom: number
  radius: number
}

/**
 * Where the host's cursor may be seen (crates/host cursor.rs): `corners`, the CarPlay area's black corners'
 * rectangle and radius, a little inside the frame (`frame`, the page's size) and its corners as round less that;
 * the whole frame (square, that little inside) while the screen is handed to the truck (`handedOver`: WestPlay's
 * button is over the truck's screen). Owner 2026-11-03: the cursor goes under the black corners at all times.
 */
export function cursorArea(
  corners: RoundedArea,
  frame: { width: number; height: number },
  handedOver: boolean,
): RoundedArea {
  const edge = handedOver ? { left: 0, top: 0, right: frame.width, bottom: frame.height, radius: 0 } : corners
  const left = Math.max(edge.left, CURSOR_INSET)
  const top = Math.max(edge.top, CURSOR_INSET)
  const right = Math.min(edge.right, frame.width - CURSOR_INSET)
  const bottom = Math.min(edge.bottom, frame.height - CURSOR_INSET)
  return { left, top, right, bottom, radius: Math.max(0, edge.radius - CURSOR_INSET) }
}
