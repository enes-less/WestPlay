import { afterEach, describe, expect, it, vi } from 'vitest'
import type { HostMessage, PageMessage } from '../../shared/host'

/** A stand-in for WebView2's `chrome.webview`: what the page posts, and a way to send it host messages. */
function fakeWebView() {
  const posted: PageMessage[] = []
  const listeners = new Set<(event: { data: HostMessage }) => void>()
  return {
    posted,
    send: (data: HostMessage) => listeners.forEach((listener) => listener({ data })),
    bridge: {
      addEventListener: (_: 'message', listener: (event: { data: HostMessage }) => void) => listeners.add(listener),
      removeEventListener: (_: 'message', listener: (event: { data: HostMessage }) => void) =>
        listeners.delete(listener),
      postMessage: (message: PageMessage) => posted.push(message),
    },
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.resetModules()
})

describe('connectHost', () => {
  it('tells the host whether WestPlay is open, and opens or closes it when the host asks', async () => {
    const webview = fakeWebView()
    vi.stubGlobal('window', { chrome: { webview: webview.bridge } })
    const { connectHost, useCabin } = await import('./store')
    const westplay = () => webview.posted.filter((message) => message.type === 'westplay')

    const disconnect = connectHost(() => 0)
    // As it connects: ready (so the host sends its state again), then open.
    expect(webview.posted[0]).toEqual({ type: 'ready' })
    expect(westplay()).toEqual([{ type: 'westplay', open: true }])

    // The companion's switch: closed, and the page says so.
    webview.send({ type: 'westplay', open: false })
    expect(useCabin.getState().truckScreen).toBe(true)
    expect(westplay().at(-1)).toEqual({ type: 'westplay', open: false })

    // Asked for what it already is: nothing to report.
    webview.send({ type: 'westplay', open: false })
    expect(westplay()).toHaveLength(2)

    // Opened in the cab (its WestPlay button): reported too.
    useCabin.getState().setTruckScreen(false)
    expect(useCabin.getState().truckScreen).toBe(false)
    expect(westplay().at(-1)).toEqual({ type: 'westplay', open: true })

    // Disconnected: no more reports.
    disconnect()
    useCabin.getState().setTruckScreen(true)
    expect(westplay()).toHaveLength(3)
  })

  it("follows the companion's truck strip setting", async () => {
    const webview = fakeWebView()
    vi.stubGlobal('window', { chrome: { webview: webview.bridge } })
    const { connectHost, useCabin } = await import('./store')
    const disconnect = connectHost(() => 0)
    expect(useCabin.getState().oemMode).toBe('auto')

    webview.send({ type: 'settings', truckStrip: 'off' })
    expect(useCabin.getState().oemMode).toBe('off')
    // A value this page doesn't know (a newer app) keeps the truck's strip.
    webview.send({ type: 'settings', truckStrip: 'sideways' })
    expect(useCabin.getState().oemMode).toBe('auto')
    disconnect()
  })

  it('keeps the weather the host reads, and forgets it when the host can no longer read it', async () => {
    const webview = fakeWebView()
    vi.stubGlobal('window', { chrome: { webview: webview.bridge } })
    const { connectHost, useLive } = await import('./store')
    const disconnect = connectHost(() => 0)
    expect(useLive.getState().weather).toBeNull()
    webview.send({ type: 'weather', weather: { now: 'clear', coming: 'rain' } })
    expect(useLive.getState().weather).toEqual({ now: 'clear', coming: 'rain' })
    webview.send({ type: 'weather', weather: null })
    expect(useLive.getState().weather).toBeNull()
    disconnect()
  })

  it("tells the host the field selected, and types what the player's keyboard typed", async () => {
    const webview = fakeWebView()
    vi.stubGlobal('window', { chrome: { webview: webview.bridge } })
    const { connectHost } = await import('./store')
    const { useKeyboard } = await import('../keyboard/store')
    const selections = () => webview.posted.filter((message) => message.type === 'typing')
    const disconnect = connectHost(() => 0)
    expect(selections()).toEqual([{ type: 'typing', selection: 0 }])
    let text = ''
    const field = {
      id: 'search',
      returnKind: 'search' as const,
      read: () => text,
      write: (t: string) => (text = t),
      submit: () => {},
    }
    useKeyboard.getState().attach(field, 'en')
    expect(selections().at(-1)).toEqual({ type: 'typing', selection: useKeyboard.getState().selection })
    webview.send({ type: 'typed', events: [{ text: 'Bari' }, { key: 'backspace' }] })
    expect(text).toBe('Bar')
    webview.send({ type: 'typingState', state: 'paused' })
    expect(useKeyboard.getState().physical).toBe('paused')
    disconnect()
  })
})
