import { useState } from 'react'

/**
 * More of a list as it's scrolled to its end: what the token of the answer `first` (or of the last more) gives,
 * added after it. Tied to `first`: another answer (another mood, a page asked afresh) starts again from its own
 * token, with nothing added.
 */
export function useMore<T>(
  first: object | null,
  token: string | null | undefined,
  fetch: (token: string) => Promise<{ items: T[]; more?: string }>,
): { items: T[]; asking: boolean; load(): void } {
  const [state, setState] = useState<{ of: object | null; items: T[]; token: string | null; asking: boolean } | null>(
    null,
  )
  const current =
    state && state.of === first ? state : { of: first, items: [] as T[], token: token ?? null, asking: false }
  const load = () => {
    if (!first || !current.token || current.asking) return
    const asked = current.token
    setState({ ...current, asking: true })
    fetch(asked).then(
      (page) =>
        setState((now) =>
          now && now.of === first
            ? { of: first, items: [...now.items, ...page.items], token: page.more ?? null, asking: false }
            : now,
        ),
      () => setState((now) => (now && now.of === first ? { ...now, asking: false } : now)),
    )
  }
  return { items: current.items, asking: current.asking, load }
}
