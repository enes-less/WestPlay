// WestPlay: the truck's own speakers, for a page's sound (youtube.js's video, apple_music.js's music): loaded
// before the page's own script, which makes one with `window.westplayCabSound(media)` (`media`: the element
// its sound comes from now, or none) and tells it the speakers, the driver's head and the camera as the host
// says them. Until there are speakers to play it (and a head to hear them), or a cab's walls to come
// through, the sound is the page's own.
window.westplayCabSound = (media) => {
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
  // The sound's way through: its context, the video it comes from, and the nodes of the speakers' way (kept
  // to be undone as the speakers change).
  let audio = null
  const placeHead = () => {
    if (!audio || !head) return
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
    // The sound is the page's own until there are speakers to play it (and a head to hear them), or a cab's
    // walls to come through.
    if (!video || (!audio && !(speakers && head) && !outside)) return
    if (!audio) {
      audio = { context: new AudioContext(), video: null, source: null, nodes: [], meters: null, muffle: null, wired: undefined }
    }
    const { context } = audio
    if (audio.video !== video) {
      audio.video = video
      audio.source = context.createMediaElementSource(video)
      audio.wired = undefined
    }
    if (audio.wired === speakers || (speakers && !head)) return
    audio.source.disconnect()
    for (const node of audio.nodes) node.disconnect()
    audio.nodes = []
    audio.meters = null
    audio.wired = speakers
    const keep = (node) => (audio.nodes.push(node), node)
    // Last of all, the cab's walls (open while the camera is in it).
    const walls = keep(new BiquadFilterNode(context, { type: 'lowpass', frequency: outside ? muffled().frequency : 20000, Q: Math.SQRT1_2 }))
    const through = keep(new GainNode(context, { gain: outside ? muffled().gain : 1 }))
    walls.connect(through)
    audio.muffle = { filter: walls, gain: through }
    if (!speakers) {
      audio.source.connect(walls)
      through.connect(context.destination)
      return
    }
    // Stereo always (a mono video plays on both sides), then each side, and both mixed.
    const stereo = keep(new GainNode(context, { channelCount: 2, channelCountMode: 'explicit', channelInterpretation: 'speakers' }))
    const split = keep(context.createChannelSplitter(2))
    audio.source.connect(stereo)
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
