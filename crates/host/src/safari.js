// WestPlay: Safari's pages (safari.rs), in every frame of every page, before the page's own scripts (after
// cab_sound.js). What the cab needs of a web page and nothing more:
// - its sound through the truck's own speakers (cab_sound.js) when the browser lets the page's sound processing
//   have it, paused with the game and for another of the cab's players, faded as theirs are;
// - the text field the player pressed, for the cab's keyboard to type into (the host types what it types);
// - the page's own colour at its top, for Safari's bars around it.
// The top frame speaks with the host; a frame inside it speaks with the top frame, which passes it on.
;(() => {
  const TAG = 'westplay-safari'
  const isTop = window === window.top
  const host = isTop ? window.chrome?.webview : null
  // The page sees Chrome (safari.rs, `chrome_identity`), whose `window.chrome` has no `webview`: kept for this
  // script alone.
  try {
    if (window.chrome && 'webview' in window.chrome) delete window.chrome.webview
  } catch {}

  // Up: to the host (from a frame, through the top frame). Only what this script says passes (`UP`).
  const UP = new Set(['field', 'list', 'playing', 'spectrum', 'theme', 'ready'])
  const toHost = (message) => {
    if (isTop) host?.postMessage(message)
    else
      try {
        window.top.postMessage({ [TAG]: 'up', message }, '*')
      } catch {}
  }
  // Each frame's sound playing, by the frame (the top frame's own too): the page plays while any does.
  const framesPlaying = new Map()
  let saidPlaying = null
  const tellPlaying = () => {
    // A frame taken off the page plays nothing more.
    for (const frame of framesPlaying.keys()) if (frame !== window && frame.closed) framesPlaying.delete(frame)
    const playing = [...framesPlaying.values()].some(Boolean)
    if (playing === saidPlaying) return
    saidPlaying = playing
    host?.postMessage({ playing })
  }
  // Down: to every frame inside this one (each passes it on to its own), but a site's bot check: it's left as a
  // browser has it, nothing of WestPlay's in it nor sent to it (safari.rs, `BOT_CHECK`; owner 2026-11-08:
  // Cloudflare's check failed).
  const BOT_CHECK = /(^|\.)(challenges\.cloudflare\.com|hcaptcha\.com|recaptcha\.net)$/
  const isBotCheck = (address) => {
    try {
      const url = new URL(address, location.href)
      return BOT_CHECK.test(url.hostname) || (url.hostname === 'www.google.com' && url.pathname.startsWith('/recaptcha/'))
    } catch {
      return false
    }
  }
  const toFrames = (message) => {
    const checks = new Set()
    for (const frame of document.querySelectorAll('iframe, frame')) {
      if (isBotCheck(frame.src)) checks.add(frame.contentWindow)
    }
    for (let i = 0; i < window.frames.length; i++) {
      if (checks.has(window.frames[i])) continue
      try {
        window.frames[i].postMessage({ [TAG]: 'down', message }, '*')
      } catch {}
    }
  }

  // ---------- Sound ----------
  const volume = Object.getOwnPropertyDescriptor(HTMLMediaElement.prototype, 'volume')
  const play = HTMLMediaElement.prototype.play
  // Every element seen playing here, its volume as the page set it (the cab's fade is on top: `level`).
  const media = new Set()
  const own = new WeakMap()
  let level = 1
  // The element whose sound goes to the speakers: the one that last started.
  let current = null
  const watch = (element) => {
    if (media.has(element)) return
    media.add(element)
    own.set(element, volume.get.call(element))
    volume.set.call(element, own.get(element) * level)
    for (const name of ['play', 'playing', 'pause', 'ended', 'emptied', 'volumechange']) {
      element.addEventListener(name, () => {
        if (name === 'play' || name === 'playing') current = element
        tellSound()
      })
    }
  }
  const sounding = (element) => !element.paused && !element.ended && !element.muted && own.get(element) > 0
  const tellSound = () => {
    const playing = [...media].some(sounding)
    if (isTop) {
      framesPlaying.set(window, playing)
      tellPlaying()
    } else toHost({ playing })
  }
  // The page's own volume, as it set it; the element plays it times the cab's fade.
  Object.defineProperty(HTMLMediaElement.prototype, 'volume', {
    configurable: true,
    enumerable: volume.enumerable,
    get() {
      return own.has(this) ? own.get(this) : volume.get.call(this)
    },
    set(value) {
      const asked = Number(value)
      // As the browser would (throwing for a volume out of range).
      if (!(asked >= 0 && asked <= 1)) return volume.set.call(this, value)
      own.set(this, asked)
      volume.set.call(this, asked * level)
    },
  })
  // An element made in script and never put on the page (`new Audio()`) tells the page nothing: it's watched as
  // it's played; one on the page, as it starts (an `autoplay` one too).
  HTMLMediaElement.prototype.play = function (...args) {
    watch(this)
    return play.apply(this, args)
  }
  document.addEventListener(
    'play',
    (event) => {
      if (event.target instanceof HTMLMediaElement) {
        watch(event.target)
        current = event.target
        tellSound()
      }
    },
    true,
  )

  // The elements WestPlay paused (the game away, another player started), to play on.
  let paused = []
  const MEDIA = {
    pauseMedia: () => {
      paused = [...media].filter((element) => !element.paused)
      for (const element of paused) element.pause()
    },
    resumeMedia: () => {
      for (const element of paused) play.call(element).catch(() => {})
      paused = []
    },
    level: (data) => {
      const asked = Number(data.level)
      level = asked >= 0 && asked <= 1 ? asked : 1
      for (const element of media) volume.set.call(element, own.get(element) * level)
    },
  }

  // Whether an element's sound may go to the speakers. The browser gives the page's sound processing nothing of
  // a sound from another site that site didn't allow (it plays silence there), and an element once taken stays
  // taken: only a sound the page itself could read is (its own site's, a stream the page fed it, one allowed
  // across sites); and only once the player has clicked in this frame, as before then the browser lets no
  // sound processing start here. Any other plays as the page has it.
  const readable = (element) => {
    if (!element || element.srcObject || !navigator.userActivation?.hasBeenActive) return false
    const source = element.currentSrc || element.src
    if (!source) return false
    if (source.startsWith('blob:') || source.startsWith('data:')) return true
    try {
      if (new URL(source, location.href).origin === location.origin) return true
    } catch {}
    return element.crossOrigin === 'anonymous' || element.crossOrigin === 'use-credentials'
  }
  const cab = window.westplayCabSound?.(() => (readable(current) ? current : null), (said) => toHost(said))
  // Nothing of WestPlay's left for the page to see.
  delete window.westplayCabSound
  if (cab) setInterval(() => cab.tick(), 250)
  const CAB = {
    speakers: (data) => cab?.speakers(data.speakers),
    head: (data) => cab?.head(data),
    outside: (data) => cab?.outside(data.on, data.distance),
    equalizer: (data) => cab?.equalizer(data),
    spectrum: (data) => cab?.spectrum(data.on),
  }
  const act = (message) => {
    const action = message?.action
    if (action === 'pick') pick(message)
    else if (CAB[action]) CAB[action](message)
    else MEDIA[action]?.(message)
  }

  // ---------- Messages between the frames and the host ----------
  host?.addEventListener('message', ({ data }) => {
    act(data)
    toFrames(data)
  })
  window.addEventListener('message', (event) => {
    const data = event.data
    if (!data || typeof data !== 'object' || !(TAG in data)) return
    if (data[TAG] === 'down' && event.source === window.parent && !isTop) {
      act(data.message)
      toFrames(data.message)
    } else if (data[TAG] === 'up' && isTop) {
      const message = data.message
      if (!message || typeof message !== 'object') return
      const keys = Object.keys(message)
      if (keys.length !== 1 || !UP.has(keys[0])) return
      if (keys[0] === 'playing') {
        framesPlaying.set(event.source, Boolean(message.playing))
        tellPlaying()
      } else if (keys[0] === 'list') {
        // Where the frame it's in is on this page (a frame inside another frame: not known here, so none).
        const list = message.list
        if (list && list.rect) {
          const frame = [...document.querySelectorAll('iframe, frame')].find((f) => f.contentWindow === event.source)
          if (frame) {
            const at = frame.getBoundingClientRect()
            list.rect = {
              x: list.rect.x + at.left + frame.clientLeft,
              y: list.rect.y + at.top + frame.clientTop,
              width: list.rect.width,
              height: list.rect.height,
            }
          } else list.rect = null
        }
        host?.postMessage({ list })
      } else if (keys[0] !== 'ready' && keys[0] !== 'theme') host?.postMessage(message)
    }
  })

  // ---------- Google's "download Chrome" ----------
  // Google's home asks, now and then, to download Chrome (a sheet over the page: its home's promotion, whatever the
  // browser says it is; owner 2026-11-08: "huge red flag that should be blocked"): never shown.
  if (isTop && /(^|\.)google\.[a-z.]+$/.test(location.hostname)) {
    const style = document.createElement('style')
    style.textContent =
      '[data-async-type="hpba"], g-bottom-sheet[aria-describedby="promo_desc_id"] { display: none !important }'
    const add = () => (document.head ?? document.documentElement)?.appendChild(style)
    if (document.documentElement) add()
    else document.addEventListener('readystatechange', add, { once: true })
  }

  // ---------- Drop-down lists and pickers ----------
  // The browser's own list of a drop-down (and a colour or date field's picker, a file field's dialog) is a window of
  // its own on the PC's screen, not on the page: a list pressed is the cab's to show instead (iPad Safari shows its
  // own), the picked option put back here; a picker or a file's dialog isn't opened (a cab has no files to give).
  let shownList = null
  const listOf = (select) => {
    const rect = select.getBoundingClientRect()
    const options = [...select.options].map((option) => ({
      label: (option.label || option.text || '').trim(),
      group: option.parentElement instanceof HTMLOptGroupElement ? option.parentElement.label : '',
      disabled: option.disabled || (option.parentElement instanceof HTMLOptGroupElement && option.parentElement.disabled),
    }))
    return {
      id: `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`,
      options,
      selected: select.selectedIndex,
      rect: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
    }
  }
  const PICKERS = new Set(['color', 'date', 'datetime-local', 'file', 'month', 'time', 'week'])
  document.addEventListener(
    'mousedown',
    (event) => {
      if (event.button !== 0) return
      const pressed = focused(event)
      const select = pressed?.closest?.('select')
      if (select && !select.multiple && select.size <= 1 && !select.disabled) {
        event.preventDefault()
        select.focus({ preventScroll: true })
        const list = listOf(select)
        shownList = { id: list.id, select }
        toHost({ list })
      } else {
        // Pressed anywhere else on the page: the list shown put away, as a list is by a press outside it.
        shownList = null
        toHost({ list: null })
      }
    },
    true,
  )
  document.addEventListener(
    'click',
    (event) => {
      const pressed = focused(event)
      if (pressed instanceof HTMLInputElement && PICKERS.has(pressed.type.toLowerCase())) event.preventDefault()
    },
    true,
  )
  const pick = (message) => {
    if (!shownList || shownList.id !== message.list) return
    const { select } = shownList
    shownList = null
    const index = Number(message.index)
    if (!(index >= 0 && index < select.options.length) || select.options[index].disabled) return
    if (select.selectedIndex === index) return
    select.selectedIndex = index
    select.dispatchEvent(new Event('input', { bubbles: true, composed: true }))
    select.dispatchEvent(new Event('change', { bubbles: true }))
  }

  // ---------- The text field pressed ----------
  const TEXT = new Set(['text', 'search', 'email', 'url', 'tel', 'password', 'number', ''])
  const fieldOf = (element) => {
    if (!element) return null
    if (element.isContentEditable) return { kind: 'text', value: element.innerText ?? '' }
    if (element instanceof HTMLTextAreaElement) return { kind: 'text', value: element.value }
    if (element instanceof HTMLInputElement && TEXT.has(element.type.toLowerCase())) {
      const kind = element.type.toLowerCase() || 'text'
      // A password's own letters stay in the page: the cab's keyboard needs only how many there are.
      const value = kind === 'password' ? '•'.repeat(element.value.length) : element.value
      return { kind, value }
    }
    return null
  }
  const focused = (event) => event.composedPath?.()[0] ?? event.target
  // Each press of a field counted (`n`), so a field pressed again (its keyboard put away meanwhile) is told again.
  let presses = 0
  const tellField = (element) => {
    const field = fieldOf(element)
    toHost({ field: field && { ...field, n: ++presses } })
  }
  document.addEventListener('focusin', (event) => tellField(focused(event)), true)
  let focusedAtPress = null
  document.addEventListener('pointerdown', () => (focusedAtPress = document.activeElement), true)
  document.addEventListener(
    'click',
    () => {
      const element = document.activeElement
      if (element && element === focusedAtPress && fieldOf(element)) tellField(element)
    },
    true,
  )
  document.addEventListener(
    'focusout',
    (event) => {
      // Another field of this page taking the focus says so itself.
      if (!fieldOf(event.relatedTarget)) toHost({ field: null })
    },
    true,
  )
  // The cab's keyboard rises over the page's lower part as a field is pressed (nothing moves for it): a field down
  // there scrolls up into the page's upper part, as an iPad's Safari keeps it in sight.
  const KEPT_ABOVE = 0.4
  document.addEventListener(
    'focusin',
    (event) => {
      const element = focused(event)
      if (!fieldOf(element)) return
      const box = element.getBoundingClientRect()
      if (box.bottom <= innerHeight * KEPT_ABOVE) return
      element.scrollIntoView({ block: 'start', inline: 'nearest' })
      if (isTop) window.scrollBy(0, -innerHeight * 0.15)
    },
    true,
  )

  // ---------- The page's colour (the top frame's) ----------
  if (isTop) {
    // The colour showing at the page's top edge, as iPad Safari tints its bars: the first painted background under
    // the top edge's middle (a site's header, whatever it turns as the page changes: YouTube's black over a video,
    // owner 2026-11-09), else the page's `theme-color`, else its own background.
    const painted = (colour) => colour && colour !== 'transparent' && !/rgba\(.*,\s*0\)$/.test(colour)
    const atTop = () => {
      let element = document.elementFromPoint(Math.floor(innerWidth / 2), 1)
      while (element) {
        const background = getComputedStyle(element).backgroundColor
        if (painted(background)) return background
        element = element.parentElement
      }
      return ''
    }
    const pageColour = () => {
      const top = atTop()
      if (top) return top
      const meta = [...document.querySelectorAll('meta[name="theme-color"]')].find(
        (tag) => !tag.media || matchMedia(tag.media).matches,
      )
      const said = meta?.content?.trim()
      if (said) return said
      for (const element of [document.body, document.documentElement]) {
        const background = element && getComputedStyle(element).backgroundColor
        if (painted(background)) return background
      }
      return ''
    }
    // The page's own ground: what the cab draws at its edges takes it (a dark page's edges never edged in white).
    const pageGround = () => {
      for (const element of [document.documentElement, document.body]) {
        const background = element && getComputedStyle(element).backgroundColor
        if (painted(background)) return background
      }
      return ''
    }
    let saidTheme = null
    let saidGround = null
    const theme = () => {
      const colour = pageColour()
      if (colour !== saidTheme) {
        saidTheme = colour
        host?.postMessage({ theme: colour })
      }
      const ground = pageGround() || colour
      if (ground !== saidGround) {
        saidGround = ground
        host?.postMessage({ ground })
      }
    }
    // Looked at again soon after the page changes (its own navigation, a header turning colour), scrolls or settles;
    // once in a while anyway.
    let soon = 0
    const lookSoon = () => {
      if (soon) return
      soon = setTimeout(() => {
        soon = 0
        theme()
      }, 150)
    }
    new MutationObserver(lookSoon).observe(document, { subtree: true, childList: true, attributes: true })
    addEventListener('scroll', lookSoon, { passive: true, capture: true })
    addEventListener('popstate', lookSoon)
    addEventListener('transitionend', lookSoon, true)
    document.addEventListener('DOMContentLoaded', theme)
    window.addEventListener('load', theme)
    setInterval(theme, 1000)

    // The cab turned light or dark (its headlights, its time of day, the player): the page sees it at once as the
    // system's (`prefers-color-scheme`); one that doesn't follow it as it runs (one that chose its colours as it
    // loaded) is loaded again, as the host decides (owner 2026-11-09: "website theme should be asked to reload. if
    // possible. if not, reload it"). Its colour's darkness told a moment after the change, and as it loads.
    const darkness = () => {
      const rgb = (pageColour().match(/[\d.]+/g) ?? []).map(Number)
      if (rgb.length < 3) return null
      const [r, g, b] = rgb.map((c) => c / 255)
      return 0.2126 * r + 0.7152 * g + 0.0722 * b < 0.5
    }
    const scheme = matchMedia('(prefers-color-scheme: dark)')
    const tellScheme = (changed) => {
      const dark = darkness()
      if (dark !== null) host?.postMessage({ follows: dark === scheme.matches, changed, site: location.host })
    }
    scheme.addEventListener('change', () => setTimeout(() => tellScheme(true), 800))
    window.addEventListener('load', () => setTimeout(() => tellScheme(false), 800))
    // As a phone's page (every page is), how far it's scaled in its window (one made for no particular width), for
    // the mouse.
    let saidScale = 1
    const scale = () => {
      const now = Math.round((window.visualViewport?.scale ?? 1) * 10000) / 10000
      if (now === saidScale) return
      saidScale = now
      host?.postMessage({ scale: now })
    }
    window.visualViewport?.addEventListener('resize', scale)
    window.addEventListener('load', scale)
    setInterval(scale, 1000)
    // Ready (a page loaded anew: the host tells it the cab again), and its site (the width its pages need).
    host?.postMessage({ ready: true, site: location.host })
  }
})()
