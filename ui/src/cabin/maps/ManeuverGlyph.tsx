import type { Step } from './guidance'

/**
 * A turn's arrow, drawn in-house in the spirit of CarPlay's: thick rounded strokes on a 48-unit square,
 * the way in from the bottom. A turn points where the road actually goes (its angle); a fork or an exit
 * shows the way not taken faintly; a roundabout shows its ring, the way round to the exit bright (round
 * anticlockwise, driving on the right), and the exit where it is.
 */
export function ManeuverGlyph({ step, size }: { step: Step; size: number }) {
  return (
    <svg viewBox="0 0 48 48" width={size} height={size} aria-hidden className="maneuver-glyph">
      <g fill="none" stroke="currentColor" strokeWidth={5} strokeLinecap="round" strokeLinejoin="round">
        {glyph(step)}
      </g>
    </svg>
  )
}

const FAINT = 0.35
/** How far an arrowhead's tip reaches past its line's end, and the square's edges a tip stays inside (its
 * stroke reaching a little further). */
const ARROW_TIP = 3
const EDGE_NEAR = 2.5
const EDGE_FAR = 45.5

/** A unit direction for an angle (degrees, clockwise from straight up). */
function along(angle: number): [number, number] {
  const radians = (angle * Math.PI) / 180
  return [Math.sin(radians), -Math.cos(radians)]
}

/** A line from `from` to `to`, ending in an arrowhead pointing along it. */
function arrow(from: [number, number], to: [number, number], opacity = 1) {
  const length = Math.hypot(to[0] - from[0], to[1] - from[1]) || 1
  const d: [number, number] = [(to[0] - from[0]) / length, (to[1] - from[1]) / length]
  const across: [number, number] = [-d[1], d[0]]
  const tip: [number, number] = [to[0] + d[0] * ARROW_TIP, to[1] + d[1] * ARROW_TIP]
  const back = (k: number, side: number): string =>
    `${to[0] - d[0] * k + across[0] * side} ${to[1] - d[1] * k + across[1] * side}`
  return (
    <g opacity={opacity}>
      <path d={`M${from[0]} ${from[1]} L${to[0] - d[0] * ARROW_TIP} ${to[1] - d[1] * ARROW_TIP}`} />
      <path d={`M${tip[0]} ${tip[1]} L${back(5, 6.5)} L${back(5, -6.5)} Z`} fill="currentColor" strokeWidth={2} />
    </g>
  )
}

function glyph(step: Step) {
  if (step.kind === 'arrive') {
    return (
      <>
        <path d="M15 43V7" />
        <path d="M15 8h20l-5 8 5 8H15" fill="currentColor" strokeWidth={3} />
      </>
    )
  }
  const { turn, angle } = step.maneuver
  const stem = <path d="M24 44V26" />
  switch (turn) {
    case 'u-turn':
      return (
        <>
          <path d="M31 44V21a7 7 0 0 0-14 0v6" />
          {arrow([17, 24], [17, 33])}
        </>
      )
    case 'keep-left':
    case 'keep-right': {
      const side = turn === 'keep-right' ? 1 : -1
      return (
        <>
          {stem}
          {arrow([24, 26], [24 - side * 9, 9], FAINT)}
          {arrow([24, 26], [24 + side * 9, 9])}
        </>
      )
    }
    case 'exit-left':
    case 'exit-right': {
      const side = turn === 'exit-right' ? 1 : -1
      return (
        <>
          {stem}
          {arrow([24, 26], [24, 6], FAINT)}
          {arrow([24, 30], [24 + side * 14, 12])}
        </>
      )
    }
    case 'roundabout': {
      // Round anticlockwise from the bottom of the ring to the exit, then out along it. A way out (nearly)
      // back where it came from is drawn well clear of the way in.
      const out = angle > 145 || angle < -145 ? -145 : angle
      const [cx, cy, r] = [24, 23, 8.5]
      const [dx, dy] = along(out)
      const exitAt: [number, number] = [cx + r * dx, cy + r * dy]
      const large = out < 0 ? 1 : 0
      // As long as the arm can be with its arrowhead's tip still inside the square.
      const room = (from: number, d: number) =>
        d > 0 ? (EDGE_FAR - from) / d : d < 0 ? (EDGE_NEAR - from) / d : Infinity
      const arm = Math.min(11, room(exitAt[0], dx) - ARROW_TIP, room(exitAt[1], dy) - ARROW_TIP)
      return (
        <>
          <circle cx={cx} cy={cy} r={r} opacity={FAINT} />
          <path d={`M24 44V${cy + r} A${r} ${r} 0 ${large} 0 ${exitAt[0]} ${exitAt[1]}`} />
          {arrow(exitAt, [exitAt[0] + dx * arm, exitAt[1] + dy * arm])}
        </>
      )
    }
    case 'sharp-left':
    case 'sharp-right': {
      // Back down beside the way in: the way in moved over to the other side, so the two stay apart.
      const side = turn === 'sharp-right' ? 1 : -1
      const [dx, dy] = along(angle)
      const x = 24 - side * 9
      return (
        <>
          <path d={`M${x} 44V12`} />
          {arrow([x, 12], [x + dx * 22, 12 + dy * 22])}
        </>
      )
    }
    default: {
      // A turn, pointing the way the road goes.
      const [dx, dy] = along(angle)
      return (
        <>
          {stem}
          {arrow([24, 26], [24 + dx * 15, 26 + dy * 15])}
        </>
      )
    }
  }
}
