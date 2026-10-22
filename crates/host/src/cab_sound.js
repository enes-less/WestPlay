// WestPlay: the truck's own speakers, for a page's sound (youtube.js's video, apple_music.js's music): loaded
// before the page's own script, which makes one with `window.westplayCabSound(media, say)` (`media`: the
// element its sound comes from now, or none; `say`: its way to the host, for the spectrum it measures) and
// tells it the speakers, the driver's head and the camera as the host says them. Until there are speakers to
// play it (and a head to hear them), or a cab's walls to come through, the sound is the page's own.
window.westplayCabSound = (media, say) => {
  // The truck's own speakers: the video's sound split as a car's system splits it (each side's tweeter the
  // highs, its door woofer the rest above the bass, the bass units the bass of both sides), each played from
  // where it sits in the cab, heard from the driver's head (the browser's 3D sound, HRTF). The cab's space
  // is the game's, x right, y up, z back: Web Audio's own. No speakers (a truck they aren't known for): the
  // sound as it is. Each split is two filters in a row (Linkwitz-Riley), so the parts add up to the whole.
  // A speaker's band: from `band[0]` Hz up to `band[1]` (0: no limit that way); and its level, in dB.
  // A speaker's sound falls with its distance from the head past this, as in a cab (metres): leaning to a
  // door, its speaker is much louder. The whole is made up so that from where the head is as the speakers
  // are placed (the driver's seat: the host tells the head first) it's as loud as the sound was.
  const NEAR = 0.3
  const fall = (at, from) => NEAR / Math.max(NEAR, Math.hypot(at[0] - from.x, at[1] - from.y, at[2] - from.z))
  let speakers = null
  let head = null
  // The camera outside the cab: the sound through its closed walls, only its bass, at once; quieter and
  // duller the further away, and from 16 m it fades out, silent at 20 m (`distance`: from the driver's head,
  // metres; 0: not known). Each row: distance (m), level (dB), and the lowpass (Hz, 250 × √(4 / distance));
  // between rows both change smoothly with the distance (as heard: by its doublings).
  let outside = false
  let distance = 0
  const MUFFLE = [
    [4, -6, 250],
    [5, -9, 224],
    [6, -12, 204],
    [8, -16, 177],
    [16, -24, 125],
  ]
  const SILENT = 20
  const muffled = () => {
    const last = MUFFLE.at(-1)
    const d = Math.min(Math.max(MUFFLE[0][0], distance), last[0])
    const after = Math.max(1, MUFFLE.findIndex(([at]) => at >= d))
    const [from, to] = [MUFFLE[after - 1], MUFFLE[after]]
    const t = Math.log2(d / from[0]) / Math.log2(to[0] / from[0])
    const level = from[1] + (to[1] - from[1]) * t
    const frequency = from[2] * (to[2] / from[2]) ** t
    const fade = Math.min(1, Math.max(0, (SILENT - distance) / (SILENT - last[0])))
    return { frequency, gain: 10 ** (level / 20) * fade }
  }
  const muffle = (cut) => {
    if (!audio?.muffle) return
    const { currentTime } = audio.context
    const { frequency, gain } = outside ? muffled() : { frequency: 20000, gain: 1 }
    // In or out at once, as the game cuts from one camera to the other; further or nearer as the camera moves.
    for (const [param, value] of [[audio.muffle.filter.frequency, frequency], [audio.muffle.gain.gain, gain]]) {
      if (cut) param.setValueAtTime(value, currentTime)
      else param.setTargetAtTime(value, currentTime, 0.05)
    }
  }
  // The player's tone (the cab's Settings › Sound), as a car stereo's: bass, mid and treble, dB each, on all
  // the sound before it's split to the speakers. Each band a filter (the bass a shelf under 100 Hz, the mids a
  // broad bell round 1 kHz, the treble a shelf over 8 kHz); the whole turned down by the most any band lifts, so
  // a boost shapes the sound without pushing it past full. Set as it's changed, gliding (no click).
  let equalizer = { bass: 0, mid: 0, treble: 0 }
  const TONE = [
    ['bass', { type: 'lowshelf', frequency: 100 }],
    ['mid', { type: 'peaking', frequency: 1000, Q: 0.7 }],
    ['treble', { type: 'highshelf', frequency: 8000 }],
  ]
  const flat = () => TONE.every(([band]) => !equalizer[band])
  const setTone = (atOnce) => {
    if (!audio?.tone) return
    const { currentTime } = audio.context
    const set = (param, value) => (atOnce ? param.setValueAtTime(value, currentTime) : param.setTargetAtTime(value, currentTime, 0.05))
    TONE.forEach(([band], i) => set(audio.tone.bands[i].gain, equalizer[band]))
    const lift = Math.max(0, ...TONE.map(([band]) => equalizer[band]))
    set(audio.tone.trim.gain, 10 ** (-lift / 20))
  }
  // The music's spectrum, while the cab's Equalizer asks for it (`spectrum(true)`): BARS bars from LOWEST to
  // HIGHEST Hz, spaced by octaves (as the ear hears them), each its loudest bin's loudness (0 to 255), measured
  // after the tone (so the curve shapes it) and said to the host (`say`) every SPECTRUM_EVERY ms while the music
  // plays; stopped or paused, said once silent (the cab's bars settle into dots).
  const BARS = 48
  const LOWEST = 40
  const HIGHEST = 16000
  const SPECTRUM_EVERY = 50
  let spectrumWanted = false
  let spectrumTimer = null
  let saidSilent = false
  const sayQuiet = () => {
    if (!saidSilent) say?.({ spectrum: new Array(BARS).fill(0) })
    saidSilent = true
  }
  const saySpectrum = () => {
    const analyser = audio?.analyser
    const video = audio?.video
    if (!analyser || !video || video.paused || video.ended) return sayQuiet()
    const data = new Uint8Array(analyser.frequencyBinCount)
    analyser.getByteFrequencyData(data)
    const perBin = audio.context.sampleRate / 2 / data.length
    const bars = Array.from({ length: BARS }, (_, i) => {
      const from = LOWEST * (HIGHEST / LOWEST) ** (i / BARS)
      const to = LOWEST * (HIGHEST / LOWEST) ** ((i + 1) / BARS)
      const first = Math.floor(from / perBin)
      const last = Math.max(first + 1, Math.ceil(to / perBin))
      let loudest = 0
      for (let bin = first; bin < last; bin++) loudest = Math.max(loudest, data[bin] ?? 0)
      // Music is quieter the higher it goes: lifted a little up there, so the highs show.
      return loudest === 0 ? 0 : Math.min(255, Math.round(loudest + (i / BARS) * 40))
    })
    say?.({ spectrum: bars })
    saidSilent = bars.every((bar) => bar === 0)
  }
  // The sound's way through: its context, the video it comes from, and the nodes of the speakers' way (kept
  // to be undone as the speakers change).
  let audio = null
  // Each element's way into the context, kept: an element can be taken in once only.
  const sources = new WeakMap()
  // The head as last placed. A head tracker never holds still: its noise (a few tenths of a degree, many times
  // a second) kept the 3D sound swapping its filters, a faint crackle all the time (-51 dB under a tone,
  // measured; none with the head still). So the head moves the sound only once it has turned a degree or moved
  // 2 cm since; while it really turns, the faint swapping stays (the browser's own, about -50 dB).
  const STILL_TURN = 1 / 360
  const STILL_MOVE = 0.02
  let placed = null
  const placeHead = () => {
    if (!audio || !head) return
    const turn = (a, b) => Math.abs(((a - b + 1.5) % 1) - 0.5)
    if (
      placed &&
      turn(head.heading, placed.heading) < STILL_TURN &&
      Math.abs(head.pitch - placed.pitch) < STILL_TURN &&
      Math.hypot(head.x - placed.x, head.y - placed.y, head.z - placed.z) < STILL_MOVE
    )
      return
    placed = { ...head }
    const { listener, currentTime } = audio.context
    const across = head.heading * 2 * Math.PI
    const up = head.pitch * 2 * Math.PI
    const values = {
      positionX: head.x,
      positionY: head.y,
      positionZ: head.z,
      forwardX: -Math.sin(across) * Math.cos(up),
      forwardY: Math.sin(up),
      forwardZ: -Math.cos(across) * Math.cos(up),
      upX: Math.sin(across) * Math.sin(up),
      upY: Math.cos(up),
      upZ: Math.cos(across) * Math.sin(up),
    }
    for (const [name, value] of Object.entries(values)) listener[name].setTargetAtTime(value, currentTime, 0.03)
  }
  const wire = () => {
    const video = media()
    // The sound is the page's own until there are speakers to play it (and a head to hear them), a cab's walls
    // to come through, a tone to give it, or its spectrum to measure.
    if (!video || (!audio && !(speakers && head) && !outside && flat() && !spectrumWanted)) return
    if (!audio) {
      audio = { context: new AudioContext(), video: null, source: null, nodes: [], meters: null, muffle: null, wired: undefined }
    }
    const { context } = audio
    if (audio.video !== video) {
      // An element is taken into the context only once, ever (a second time throws, and its sound would stay
      // in the first way, unheard): one the page plays again (a player switching between its elements, as
      // around a pause) gets its own way back.
      let source = sources.get(video)
      if (!source) {
        source = context.createMediaElementSource(video)
        sources.set(video, source)
      }
      audio.video = video
      audio.source = source
      audio.wired = undefined
    }
    if (audio.wired === speakers || (speakers && !head)) return
    audio.source.disconnect()
    for (const node of audio.nodes) node.disconnect()
    audio.nodes = []
    audio.meters = null
    audio.wired = speakers
    const keep = (node) => (audio.nodes.push(node), node)
    // First of all, the tone.
    const bands = TONE.map(([, options]) => keep(new BiquadFilterNode(context, options)))
    const trim = keep(new GainNode(context))
    bands.reduce((from, to) => (from.connect(to), to)).connect(trim)
    audio.source.connect(bands[0])
    audio.tone = { bands, trim }
    setTone(true)
    // Beside it, the spectrum's ear (measuring, playing nothing).
    audio.analyser = keep(new AnalyserNode(context, { fftSize: 4096, smoothingTimeConstant: 0.5, minDecibels: -80, maxDecibels: -22 }))
    trim.connect(audio.analyser)
    // Last of all, the cab's walls (open while the camera is in it).
    const walls = keep(new BiquadFilterNode(context, { type: 'lowpass', frequency: outside ? muffled().frequency : 20000, Q: Math.SQRT1_2 }))
    const through = keep(new GainNode(context, { gain: outside ? muffled().gain : 1 }))
    walls.connect(through)
    audio.muffle = { filter: walls, gain: through }
    if (!speakers) {
      trim.connect(walls)
      through.connect(context.destination)
      return
    }
    // Stereo always (a mono video plays on both sides), then each side, and both mixed.
    const stereo = keep(new GainNode(context, { channelCount: 2, channelCountMode: 'explicit', channelInterpretation: 'speakers' }))
    const split = keep(context.createChannelSplitter(2))
    trim.connect(stereo)
    stereo.connect(split)
    const channels = { left: keep(context.createGain()), right: keep(context.createGain()), both: keep(new GainNode(context, { gain: 0.5 })) }
    split.connect(channels.left, 0)
    split.connect(channels.right, 1)
    split.connect(channels.both, 0)
    split.connect(channels.both, 1)
    // Made up for the distance as the speakers' power adds up at the seat (each plays its own part of the
    // sound): their loudnesses there, squared, summed. (Their average made it up far too much: a loud song
    // went past full, and the limiter squashed it all the time.)
    const seat = Math.sqrt(
      speakers.reduce((sum, speaker) => sum + (fall(speaker.at, head) * 10 ** ((speaker.level ?? 0) / 20)) ** 2, 0),
    )
    // With 2 dB to spare (a loud song's peaks from the seat stay under the limiter).
    const HEADROOM = 10 ** (-2 / 20)
    const out = keep(new GainNode(context, { gain: Math.min(6, Math.max(1, HEADROOM / seat)) }))
    // Never past full: a head right by a speaker is loud, and a limiter keeps it clean (only then).
    const limiter = keep(new DynamicsCompressorNode(context, { threshold: -3, knee: 0, ratio: 20, attack: 0.003, release: 0.1 }))
    audio.limiter = limiter
    // The walls after the limiter: it would make up what they take.
    out.connect(limiter)
    limiter.connect(walls)
    through.connect(context.destination)
    // A meter on each ear.
    const ears = keep(context.createChannelSplitter(2))
    through.connect(ears)
    audio.meters = [0, 1].map((ear) => {
      const meter = keep(new AnalyserNode(context, { fftSize: 2048 }))
      ears.connect(meter, ear)
      return meter
    })
    for (const speaker of speakers) {
      let node = channels[speaker.channel]
      const [from, to] = speaker.band
      const filters = [...(from > 0 ? [['highpass', from]] : []), ...(to > 0 ? [['lowpass', to]] : [])]
      for (const [type, frequency] of filters) {
        for (let i = 0; i < 2; i++) {
          const filter = keep(new BiquadFilterNode(context, { type, frequency, Q: Math.SQRT1_2 }))
          node.connect(filter)
          node = filter
        }
      }
      const level = keep(new GainNode(context, { gain: 10 ** ((speaker.level ?? 0) / 20) }))
      node.connect(level)
      node = level
      const [x, y, z] = speaker.at
      const panner = keep(
        new PannerNode(context, {
          panningModel: 'HRTF',
          distanceModel: 'inverse',
          refDistance: NEAR,
          rolloffFactor: 1,
          positionX: x,
          positionY: y,
          positionZ: z,
        }),
      )
      node.connect(panner)
      panner.connect(out)
    }
    // A way made anew (its context's listener perhaps new too): the head placed whatever it last was.
    placed = null
    placeHead()
  }
  // How loud the speakers play now in each ear, 0 to 1 (the loudness of the last moment of sound), and how
  // much the limiter is taking off (dB, 0 or less); or none.
  const cabLevel = () =>
    audio?.meters && [...audio.meters.map((meter) => {
      const samples = new Float32Array(meter.fftSize)
      meter.getFloatTimeDomainData(samples)
      let sum = 0
      for (const sample of samples) sum += sample * sample
      return Math.round(Math.sqrt(sum / samples.length) * 10000) / 10000
    }), Math.round((audio.limiter?.reduction ?? 0) * 100) / 100]

  return {
    speakers(list) {
      speakers = list ?? null
      try {
        wire()
      } catch {}
    },
    head(told) {
      head = told
      placeHead()
      try {
        wire()
      } catch {}
    },
    spectrum(on) {
      spectrumWanted = Boolean(on)
      clearInterval(spectrumTimer)
      spectrumTimer = null
      saidSilent = false
      if (spectrumWanted) spectrumTimer = setInterval(saySpectrum, SPECTRUM_EVERY)
      else sayQuiet()
      try {
        wire()
      } catch {}
    },
    equalizer(told) {
      equalizer = Object.fromEntries(TONE.map(([band]) => [band, Number(told?.[band]) || 0]))
      setTone(false)
      try {
        wire()
      } catch {}
    },
    outside(on, far) {
      const cut = outside !== on
      outside = on
      distance = far ?? 0
      try {
        wire()
        muffle(cut)
      } catch {}
    },
    // Each pass: the speakers' way on the element there is now (the page may make a new one); the sound's
    // context woken if it sleeps.
    tick() {
      try {
        wire()
        if (audio?.context.state === 'suspended') audio.context.resume()
      } catch {}
    },
    level: cabLevel,
  }
}
