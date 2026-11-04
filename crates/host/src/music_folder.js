// WestPlay: Player's own player (music_folder.rs), in a WebView of its own hidden under the cabin page: the
// songs of the game's music folder (served at https://westplay.music/), played by one audio element, its sound
// through the truck's own speakers (cab_sound.js, loaded first). The cab's Player app says what to play; this
// says what plays.
;(() => {
  if (location.hostname !== 'westplay.player') return
  const post = (message) => window.chrome?.webview?.postMessage(message)
  const audio = new Audio()
  audio.preload = 'auto'
  // Its sound taken into the page's own sound processing (the speakers): served for any page, as asked.
  audio.crossOrigin = 'anonymous'
  const url = (file) => `https://westplay.music/${encodeURIComponent(file)}`

  // The songs as asked (the folder's order), the order they play in (shuffled or not), and which plays.
  let asked = []
  let queue = []
  let index = -1
  let shuffle = false
  let repeat = 'off'

  const shuffled = (files) => {
    const out = [...files]
    for (let i = out.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1))
      ;[out[i], out[j]] = [out[j], out[i]]
    }
    return out
  }
  const current = () => queue[index] ?? null

  // What plays: the song, playing or not (and loading: asked to play, its sound not come yet), where, how
  // long, the options, the volume, and where it is in the songs (`null`: nothing).
  let told = ''
  const tell = () => {
    const file = current()
    const playback = file && {
      file,
      playing: !audio.paused,
      loading: !audio.paused && audio.readyState < 3,
      position: Number(audio.currentTime) || 0,
      duration: Number.isFinite(audio.duration) ? audio.duration : 0,
      shuffle,
      repeat,
      volume: audio.volume,
      index,
      count: queue.length,
    }
    const now = JSON.stringify(playback ?? null)
    if (now === told) return
    told = now
    post({ playback: playback ?? null })
  }

  const load = (at, play) => {
    index = at
    const file = current()
    if (!file) {
      audio.removeAttribute('src')
      audio.load()
      tell()
      return
    }
    audio.src = url(file)
    if (play) audio.play().catch(() => tell())
    tell()
  }

  // Next (1) or back (-1); past the last: the first again with Repeat, else stopped at the last's start.
  const skip = (by, play = !audio.paused) => {
    if (queue.length === 0) return
    const next = index + by
    if (next >= queue.length) {
      if (repeat === 'all') return load(0, play)
      audio.pause()
      audio.currentTime = 0
      return tell()
    }
    load(Math.max(0, next), play)
  }

  const CONTROLS = {
    // The songs (file names, in the folder's order) from the one at `index`, shuffled or not; with none, play on.
    play({ files, index: at, shuffle: on }) {
      if (!Array.isArray(files)) {
        if (current()) audio.play().catch(() => tell())
        return
      }
      asked = files.filter((file) => typeof file === 'string')
      shuffle = Boolean(on)
      const first = shuffle && at == null ? asked[Math.floor(Math.random() * asked.length)] : asked[at ?? 0]
      queue = shuffle ? [first, ...shuffled(asked.filter((file) => file !== first))] : asked
      load(Math.max(0, queue.indexOf(first)), true)
    },
    pause: () => audio.pause(),
    next: () => skip(1),
    // Back: past a song's first seconds, its start.
    previous() {
      if (audio.currentTime > 3 || index <= 0) {
        audio.currentTime = 0
        tell()
      } else skip(-1)
    },
    seek({ seconds }) {
      if (Number.isFinite(seconds)) audio.currentTime = Math.max(0, seconds)
      tell()
    },
    volume({ percent }) {
      if (Number.isFinite(percent)) audio.volume = Math.min(1, Math.max(0, percent / 100))
    },
    shuffle({ on }) {
      const file = current()
      shuffle = Boolean(on)
      queue = shuffle ? [file, ...shuffled(asked.filter((each) => each !== file))].filter(Boolean) : asked
      index = file ? queue.indexOf(file) : -1
      tell()
    },
    repeat({ mode }) {
      repeat = ['off', 'all', 'one'].includes(mode) ? mode : 'off'
      tell()
    },
  }

  audio.addEventListener('ended', () => {
    if (repeat === 'one') {
      audio.currentTime = 0
      audio.play().catch(() => tell())
    } else skip(1, true)
  })
  // A song that can't be played (gone from the folder, or a kind the browser can't play): the next.
  audio.addEventListener('error', () => {
    if (current() && queue.length > 1) skip(1, true)
    else tell()
  })
  for (const name of ['play', 'pause', 'playing', 'waiting', 'loadedmetadata', 'durationchange', 'seeked', 'volumechange']) {
    audio.addEventListener(name, tell)
  }

  // The truck's own speakers (cab_sound.js): this page's audio element, told the speakers, the head, the
  // camera and the tone by the host.
  const cab = window.westplayCabSound?.(() => audio, post)
  if (cab) setInterval(() => cab.tick(), 250)
  const CAB = {
    speakers: (data) => cab?.speakers(data.speakers),
    head: (data) => cab?.head(data),
    outside: (data) => cab?.outside(data.on, data.distance),
    equalizer: (data) => cab?.equalizer(data),
    spectrum: (data) => cab?.spectrum(data.on),
  }
  window.chrome?.webview?.addEventListener('message', ({ data }) => {
    if (CAB[data?.action]) CAB[data.action](data)
    else CONTROLS[data?.action]?.(data)
  })

  // Ready (a page loaded anew: the host tells it the cab again).
  post({ ready: true })
  tell()
})()
