import { afterEach, describe, expect, it, vi } from 'vitest'
import type { HostMessage, PageMessage, Telemetry } from '../../shared/host'

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

  it('keeps the tone the host has, and hands each band set in Settings to it', async () => {
    const webview = fakeWebView()
    vi.stubGlobal('window', { chrome: { webview: webview.bridge } })
    const { connectHost, useCabin } = await import('./store')
    const disconnect = connectHost(() => 0)
    expect(useCabin.getState().equalizer).toEqual({ bass: 0, mid: 0, treble: 0 })

    // As the host keeps it; whatever it sends, each band whole dB within ±6 (none said: flat).
    webview.send({ type: 'settings', truckStrip: 'auto', equalizer: { bass: -3, mid: 2.4, treble: 40 } })
    expect(useCabin.getState().equalizer).toEqual({ bass: -3, mid: 2, treble: 6 })
    webview.send({ type: 'settings', truckStrip: 'auto' })
    expect(useCabin.getState().equalizer).toEqual({ bass: 0, mid: 0, treble: 0 })

    expect(useCabin.getState().equalizerCustom).toBe(false)

    // A band set: Custom, and the whole curve to the host, to keep and play.
    useCabin.getState().setEqualizerBand('bass', -4)
    expect(useCabin.getState().equalizer).toEqual({ bass: -4, mid: 0, treble: 0 })
    expect(useCabin.getState().equalizerCustom).toBe(true)
    expect(webview.posted.at(-1)).toEqual({ type: 'equalizer', custom: true, bass: -4, mid: 0, treble: 0 })
    useCabin.getState().setEqualizerBand('treble', 9)
    expect(webview.posted.at(-1)).toEqual({ type: 'equalizer', custom: true, bass: -4, mid: 0, treble: 6 })
    // Recommended: the curve kept, flat played.
    useCabin.getState().chooseEqualizer(false)
    expect(webview.posted.at(-1)).toEqual({ type: 'equalizer', custom: false, bass: -4, mid: 0, treble: 6 })
    webview.send({
      type: 'settings',
      truckStrip: 'auto',
      equalizer: { bass: -4, mid: 0, treble: 6 },
      equalizerCustom: true,
    })
    expect(useCabin.getState().equalizerCustom).toBe(true)
    useCabin.getState().resetEqualizer()
    expect(webview.posted.at(-1)).toEqual({ type: 'equalizer', custom: true, bass: 0, mid: 0, treble: 0 })
    disconnect()
  })

  it('powers off and forgets the telemetry as the game closes, and powers on again with the next game', async () => {
    const webview = fakeWebView()
    vi.stubGlobal('window', { chrome: { webview: webview.bridge } })
    const { connectHost, useCabin, useLive } = await import('./store')
    const disconnect = connectHost(() => 0)
    const electrics = (on: boolean) => ({ electricEnabled: on, paused: false }) as Telemetry
    webview.send({ type: 'telemetry', data: electrics(true) })
    expect(useCabin.getState().power.phase).not.toBe('off')

    // Closed, its last telemetry still saying the electrics are on: off, and nothing kept of it.
    webview.send({ type: 'game', running: false })
    expect(useCabin.getState().power.phase).toBe('off')
    expect(useLive.getState().telemetry).toBeNull()

    // The next game, electrics on as the last one left them: on again.
    webview.send({ type: 'game', running: true })
    webview.send({ type: 'telemetry', data: electrics(true) })
    expect(useCabin.getState().power.phase).not.toBe('off')
    disconnect()
  })

  it("keeps TruckersMP's players around the player, and forgets them when the game isn't under it", async () => {
    const webview = fakeWebView()
    vi.stubGlobal('window', { chrome: { webview: webview.bridge } })
    const { connectHost, useLive } = await import('./store')
    const disconnect = connectHost(() => 5)
    webview.send({ type: 'nearby', server: 4, local: 113, players: [[7, -6175.5, 2139, 0.25]] })
    expect(useLive.getState().nearby).toEqual({ server: 4, local: 113, players: [[7, -6175.5, 2139, 0.25]], at: 5 })
    // None to tell: none kept.
    webview.send({ type: 'nearby', server: null, local: null, players: [] })
    expect(useLive.getState().nearby).toBeNull()
    webview.send({ type: 'nearby', server: 4, local: 113, players: [] })
    webview.send({ type: 'truckersmp', on: false })
    expect(useLive.getState().nearby).toBeNull()
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
