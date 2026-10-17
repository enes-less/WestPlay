/**
 * The screen's power sequence, driven by the truck's electrics:
 *
 *   off ──electrics on──▶ splash ──SPLASH_MS──▶ connecting ──CONNECTING_MS──▶ on
 *    ▲                                                                       │
 *    └────────────────────────────── electrics off ◀─────────────────────────┘
 *
 * `splash` is the truck's own boot screen (brand logo); `connecting` is the phone connecting, like CarPlay
 * starting up. If WestPlay first sees the truck with the electrics already on (the app or plugin started
 * mid-drive), it skips the brand splash and only reconnects.
 *
 * The game itself hides the screen while the electrics are off, so `off` is mostly bookkeeping.
 * The pairing step ("Connect your phone?") will sit between `connecting` and `on` once there is input (M2).
 */
export type PowerPhase = 'off' | 'splash' | 'connecting' | 'on'

export interface PowerState {
  phase: PowerPhase
  /** When the current phase started (ms, from the caller's clock). */
  since: number
  /** Whether the electrics state has been observed yet. */
  observed: boolean
}

export type PowerEvent = { type: 'electrics'; on: boolean; at: number } | { type: 'tick'; at: number }

export const SPLASH_MS = 2400
export const CONNECTING_MS = 2600

export const INITIAL_POWER: PowerState = { phase: 'off', since: 0, observed: false }

export function power(state: PowerState, event: PowerEvent): PowerState {
  switch (event.type) {
    case 'electrics': {
      if (!event.on) {
        return state.phase === 'off' && state.observed ? state : { phase: 'off', since: event.at, observed: true }
      }
      if (state.phase !== 'off') return state.observed ? state : { ...state, observed: true }
      return { phase: state.observed ? 'splash' : 'connecting', since: event.at, observed: true }
    }
    case 'tick': {
      // Against the moment the phase is due (nextPhaseAt), computed the same way, so a tick sent for that
      // moment always advances it.
      const due = nextPhaseAt(state)
      if (due === null || event.at < due) return state
      return { ...state, phase: state.phase === 'splash' ? 'connecting' : 'on', since: event.at }
    }
  }
}

/** When (on the caller's clock) the current phase advances by itself, or `null` if it doesn't. */
export function nextPhaseAt(state: PowerState): number | null {
  if (state.phase === 'splash') return state.since + SPLASH_MS
  if (state.phase === 'connecting') return state.since + CONNECTING_MS
  return null
}
