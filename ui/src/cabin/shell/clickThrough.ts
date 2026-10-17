/**
 * Clicks during a screen change. While a View Transition animates, the browser shows snapshots and treats
 * every element taking part as unpainted: a click passes through the animation (`::view-transition` in
 * transitions.css) but then hits nothing, landing on the page's empty body. Like a touchscreen, though, the
 * last press must count. So such a click is given to the button that is really under it (by its layout,
 * which is already the new screen's), the sidebar's first, since it sits on top.
 *
 * Installs the handler; returns the function that removes it.
 */
export function routeClicksDuringTransitions(): () => void {
  const onClick = (event: MouseEvent) => {
    const root = document.documentElement
    if (!root.dataset.transition) return
    if (event.target !== document.body && event.target !== root) return
    const button = buttonAt(event.clientX, event.clientY)
    if (!button) return
    event.stopPropagation()
    button.click()
  }
  window.addEventListener('click', onClick, true)
  return () => window.removeEventListener('click', onClick, true)
}

/** The enabled button at a point of the page, by layout: the sidebar's first, otherwise the last in order. */
export function buttonAt(x: number, y: number): HTMLButtonElement | null {
  const contains = (button: HTMLButtonElement) => {
    const rect = button.getBoundingClientRect()
    return !button.disabled && x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
  }
  const sidebar = [...document.querySelectorAll<HTMLButtonElement>('.sidebar button')].find(contains)
  if (sidebar) return sidebar
  const all = [...document.querySelectorAll<HTMLButtonElement>('.carplay button')].filter(contains)
  return all.at(-1) ?? null
}
