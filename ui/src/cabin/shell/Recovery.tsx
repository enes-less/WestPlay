import { Component, type ErrorInfo, type ReactNode } from 'react'

/** How long the cabin stays away after it fails, ms: at first, then twice as long for each failure that
 * follows, up to the longest. */
const FIRST_RETRY_MS = 1000
const LONGEST_RETRY_MS = 30_000
/** Running this long without failing counts as recovered: the next failure waits the first time again. */
const RECOVERED_MS = 60_000

interface State {
  failed: boolean
  /** Bumped to mount the cabin afresh. */
  attempt: number
}

/**
 * Keeps one failing screen from blanking the truck's screen for good: if rendering the cabin throws, it
 * shows nothing for a moment (the page is transparent, so the game's own screen shows through), then mounts
 * the cabin afresh (its screens' own state starts over; what the host sent is kept in the stores), waiting
 * longer each time it fails again soon.
 */
export class Recovery extends Component<{ children: ReactNode }, State> {
  state: State = { failed: false, attempt: 0 }
  private failures = 0
  private lastFailure = 0
  private timer: ReturnType<typeof setTimeout> | null = null

  static getDerivedStateFromError(): Partial<State> {
    return { failed: true }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    const now = performance.now()
    if (now - this.lastFailure > RECOVERED_MS) this.failures = 0
    this.lastFailure = now
    const wait = Math.min(LONGEST_RETRY_MS, FIRST_RETRY_MS * 2 ** this.failures)
    this.failures += 1
    console.error(`cabin: a screen failed (retrying in ${wait} ms)`, error, info.componentStack)
    if (this.timer !== null) clearTimeout(this.timer)
    this.timer = setTimeout(() => {
      this.timer = null
      this.setState(({ attempt }) => ({ failed: false, attempt: attempt + 1 }))
    }, wait)
  }

  componentWillUnmount() {
    if (this.timer !== null) clearTimeout(this.timer)
  }

  render() {
    if (this.state.failed) return null
    return <RecoveryAttempt key={this.state.attempt}>{this.props.children}</RecoveryAttempt>
  }
}

/** The children, keyed by attempt so a retry mounts them afresh. */
function RecoveryAttempt({ children }: { children: ReactNode }) {
  return children
}
