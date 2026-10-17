import { previewLyrics } from '../lyricsTime'
import type {
  Answers,
  Question,
  SpotifyArtist,
  SpotifyCategory,
  SpotifyCollection,
  SpotifyItem,
  SpotifyResults,
  SpotifyTrack,
} from './spotify'

/**
 * DEVELOPMENT PREVIEW: until the web player's bridge is built, invented artists, albums and playlists answer
 * the app (no pictures: the app draws covers in their colours). Nothing here is anyone's real music.
 */

interface Artist {
  name: string
  color: string
  tags: string[]
  albums: { name: string; year: number; color: string }[]
}

const ARTISTS: Artist[] = [
  {
    name: 'Mira Vale',
    color: '#c2477a',
    tags: ['pop', 'chill'],
    albums: [
      { name: 'Blue Hour', year: 2025, color: '#3b5fc0' },
      { name: 'Paper Maps', year: 2022, color: '#d4875b' },
    ],
  },
  {
    name: 'Diesel Hearts',
    color: '#b5432e',
    tags: ['rock', 'driving'],
    albums: [
      { name: 'Fuel & Fire', year: 2024, color: '#c8361f' },
      { name: 'Weigh Station', year: 2021, color: '#6b6f2a' },
    ],
  },
  {
    name: 'Nordkapp',
    color: '#2f6f86',
    tags: ['electronic', 'focus', 'driving'],
    albums: [
      { name: 'Polar Night', year: 2026, color: '#1f4e8c' },
      { name: 'Coastline', year: 2023, color: '#2a8c7d' },
    ],
  },
  {
    name: 'Kaya Sol',
    color: '#d9a13b',
    tags: ['pop', 'dance'],
    albums: [
      { name: 'Golden Exit', year: 2025, color: '#e0a526' },
      { name: 'Wide Awake', year: 2023, color: '#9c3fb8' },
    ],
  },
  {
    name: 'The Long Haul',
    color: '#7a5233',
    tags: ['rock', 'country', 'driving'],
    albums: [
      { name: 'Westbound', year: 2024, color: '#a0612c' },
      { name: 'Mile Markers', year: 2020, color: '#55704a' },
    ],
  },
  {
    name: 'Lumen Coast',
    color: '#4e7fb3',
    tags: ['chill', 'sleep', 'focus'],
    albums: [
      { name: 'Harbour Lights', year: 2025, color: '#3a6ea8' },
      { name: 'After Rain', year: 2022, color: '#6b8f9e' },
    ],
  },
  {
    name: 'Ozan Er',
    color: '#8c3b52',
    tags: ['hip-hop', 'pop'],
    albums: [
      { name: 'Gece Yolu', year: 2026, color: '#5a2a7a' },
      { name: 'Kırmızı Işık', year: 2024, color: '#b22d3d' },
    ],
  },
  {
    name: 'Midnight Freight',
    color: '#34345c',
    tags: ['electronic', 'dance', 'driving'],
    albums: [
      { name: 'Red Eye', year: 2025, color: '#7d1f2f' },
      { name: 'Sleeper Cab', year: 2023, color: '#2b3a67' },
    ],
  },
]

const TITLES = [
  'Northbound',
  'Headlights',
  'Gravel & Gold',
  'Last Exit',
  'Slow Lanes',
  'Diesel Moon',
  'Open Road',
  'Night Ferry',
  'Static',
  'Cruise Control',
  'Low Sun',
  'Ghost Town Radio',
  'Long Way Home',
  'Tail Lights',
  'Borderline',
  'Frost on the Glass',
  'Overpass',
  'Amber Signals',
  'Cold Coffee',
  'Midnight Toll',
  'Valley Drive',
  'Silver Line',
  'Echo Valley',
  'Gearshift',
  'Rest Stop',
  'Highway Hymn',
  'Horizon',
  'Kilometres',
  'Bridges Burning Slow',
  'Dashboard Glow',
  'Lighthouse',
  'Rain on Steel',
]

function slug(text: string): string {
  return text
    .toLocaleLowerCase()
    .normalize('NFD')
    .replace(/[^a-z0-9]+/g, '')
}

const artistUri = (artist: Artist) => `spotify:artist:preview${slug(artist.name)}`

function artistItem(artist: Artist): SpotifyItem {
  return {
    uri: artistUri(artist),
    kind: 'artist',
    name: artist.name,
    by: '',
    description: '',
    image: null,
    color: artist.color,
  }
}

interface Album {
  item: SpotifyItem
  artist: Artist
  year: number
  tracks: SpotifyTrack[]
}

const ALBUMS: Album[] = ARTISTS.flatMap((artist, a) =>
  artist.albums.map((album, b) => {
    const uri = `spotify:album:preview${slug(album.name)}`
    const count = 8 + ((a + b) % 4)
    const tracks = Array.from({ length: count }, (_, t): SpotifyTrack => {
      const title = TITLES[(a * 5 + b * 11 + t * 3) % TITLES.length]
      const featuring = t % 5 === 3 ? ARTISTS[(a + 3) % ARTISTS.length] : null
      return {
        uri: `spotify:track:preview${slug(album.name)}${t}`,
        name: t === 0 ? album.name : title,
        artists: [artist, ...(featuring ? [featuring] : [])].map((who) => ({ uri: artistUri(who), name: who.name })),
        album: { uri, name: album.name },
        image: null,
        color: album.color,
        duration: 150 + ((a * 37 + b * 53 + t * 29) % 140),
        explicit: artist.tags.includes('hip-hop') && t % 3 === 1,
      }
    })
    return {
      item: {
        uri,
        kind: 'album' as const,
        name: album.name,
        by: artist.name,
        description: `${album.year} • ${artist.name}`,
        image: null,
        color: album.color,
      },
      artist,
      year: album.year,
      tracks,
    }
  }),
)

const ALL_TRACKS = ALBUMS.flatMap((album) => album.tracks)

/** A mix of songs, the same every time for the same name. */
function mix(name: string, count: number, from: (artist: Artist) => boolean = () => true): SpotifyTrack[] {
  const pool = ALBUMS.filter((album) => from(album.artist)).flatMap((album) => album.tracks)
  const seed = [...name].reduce((sum, letter) => (sum * 31 + letter.charCodeAt(0)) % 9973, 7)
  return Array.from({ length: Math.min(count, pool.length) }, (_, i) => pool[(seed + i * 7) % pool.length])
}

interface Playlist {
  item: SpotifyItem
  tracks: SpotifyTrack[]
}

function playlist(name: string, color: string, description: string, tracks: SpotifyTrack[], by = 'Spotify'): Playlist {
  return {
    item: { uri: `spotify:playlist:preview${slug(name)}`, kind: 'playlist', name, by, description, image: null, color },
    tracks,
  }
}

function dailyMix(n: number, color: string, artists: number[]): Playlist {
  const who = artists.map((a) => ARTISTS[a])
  return playlist(
    `Daily Mix ${n}`,
    color,
    `${who[0].name}, ${who[1].name} and more`,
    mix(`daily${n}`, 24, (artist) => who.includes(artist)),
  )
}

const LIKED_COUNT = 128

const LIKED: Playlist = {
  item: {
    uri: 'spotify:collection:tracks',
    kind: 'liked',
    name: 'Liked Songs',
    by: '',
    description: '',
    songs: LIKED_COUNT,
    image: null,
    color: '#5038a0',
    pinned: true,
    downloaded: true,
  },
  tracks: mix('liked', 40),
}

const PLAYLISTS: Playlist[] = [
  dailyMix(1, '#e1663b', [1, 4, 7]),
  dailyMix(2, '#3f9b6e', [0, 3, 6]),
  dailyMix(3, '#5b57d1', [2, 5, 7]),
  dailyMix(4, '#c43d7a', [3, 0, 1]),
  playlist(
    'Discover Weekly',
    '#3e6fd8',
    'Your weekly mixtape of fresh music. Enjoy new music picked for you.',
    mix('dw', 30),
  ),
  playlist('Release Radar', '#68b04a', 'Catch all the latest music from artists you follow.', mix('rr', 30)),
  playlist('On Repeat', '#e2405a', 'Songs you love right now.', mix('repeat', 30)),
  playlist(
    'Night Drive',
    '#2c2c78',
    'Keep your eyes on the road and your ears on these.',
    mix('night', 40, (a) => a.tags.includes('driving') || a.tags.includes('electronic')),
  ),
  playlist(
    'Driving Rock',
    '#c22a1e',
    'Turn it up. Rock for the open road.',
    mix('rock', 40, (a) => a.tags.includes('rock')),
  ),
  playlist(
    'Chill Hits',
    '#3fa8a3',
    'Kick back to the best new and recent chill hits.',
    mix('chill', 40, (a) => a.tags.includes('chill')),
  ),
  playlist('Road Trip', '#e0952c', 'Songs for the long way round.', mix('road', 40)),
  playlist(
    'Morning Coffee',
    '#8b5e3c',
    'Ease into the day.',
    mix('coffee', 30, (a) => a.tags.includes('chill') || a.tags.includes('pop')),
  ),
  playlist('Long haul', '#41639c', '', mix('mine', 52), 'You'),
  playlist(
    'Rock Mix',
    '#b5432e',
    'Diesel Hearts, The Long Haul and more',
    mix('rockmix', 30, (a) => a.tags.includes('rock')),
  ),
  playlist(
    'Chill Mix',
    '#4e7fb3',
    'Lumen Coast, Mira Vale and more',
    mix('chillmix', 30, (a) => a.tags.includes('chill')),
  ),
  playlist('2020s Mix', '#9c3fb8', 'Kaya Sol, Ozan Er and more', mix('2020s', 30)),
  playlist(
    'Dance Mix',
    '#dc148c',
    'Midnight Freight, Kaya Sol and more',
    mix('dance', 30, (a) => a.tags.includes('dance')),
  ),
]

const SHOWS: SpotifyItem[] = [
  ['The Fifth Wheel', 'Haulage Media', '#2b6b4f', 'Stories from the road, one driver at a time.'],
  ['Long Road Stories', 'Open Lane Audio', '#8a3b2c', 'True stories told at highway speed.'],
  ['Cab Talk', 'Cab Talk', '#3d4f8f', 'Two drivers, one cab, every topic.'],
  ['Route Notes', 'Route Notes', '#a07a1f', 'Maps, history and the places in between.'],
  ['Night Shift', 'Nightline', '#40285e', 'Company for the small hours.'],
].map(([name, by, color, description]) => ({
  uri: `spotify:show:preview${slug(name)}`,
  kind: 'show' as const,
  name,
  by,
  description,
  image: null,
  color,
}))

const byName = (name: string) => PLAYLISTS.find((p) => p.item.name === name)!.item
const album = (name: string) => ALBUMS.find((a) => a.item.name === name)!.item

/** Charts and radios, Spotify's own (signed-out Home shows them). */
const CHARTS: Playlist[] = [
  playlist('Top 50 - Global', '#2b6cb0', 'Your daily update of the most played tracks right now.', mix('top50', 50)),
  playlist('Viral 50 - Global', '#1e9e6a', 'Your daily update of the most viral tracks right now.', mix('viral50', 50)),
  playlist('Top 50 - Türkiye', '#c53030', 'Your daily update of the most played tracks right now.', mix('top50tr', 50)),
  playlist('Top Songs - Global', '#6b46c1', 'Your weekly update of the most played tracks.', mix('topweekly', 50)),
]

const RADIOS: Playlist[] = ARTISTS.slice(0, 6).map((artist) =>
  playlist(
    `${artist.name} Radio`,
    artist.color,
    `With ${ARTISTS[(ARTISTS.indexOf(artist) + 2) % ARTISTS.length].name} and more`,
    mix(`radio${artist.name}`, 30, (a) => a.tags.some((tag) => artist.tags.includes(tag))),
  ),
)

/** Spotify's Home for anyone, signed out: what's popular, no shortcuts (they're the player's own). */
function publicHome(): Answers['home'] {
  return {
    chips: [],
    shortcuts: [],
    sections: [
      {
        title: 'Popular artists',
        items: [ARTISTS[3], ARTISTS[6], ARTISTS[0], ARTISTS[1], ARTISTS[7], ARTISTS[2], ARTISTS[4]].map(artistItem),
      },
      {
        title: 'Popular albums and singles',
        items: [
          album('Golden Exit'),
          album('Gece Yolu'),
          album('Fuel & Fire'),
          album('Red Eye'),
          album('Blue Hour'),
          album('Polar Night'),
          album('Westbound'),
        ],
      },
      { title: 'Popular radio', items: RADIOS.map((r) => r.item) },
      { title: 'Featured Charts', items: CHARTS.map((c) => c.item) },
      {
        title: 'Spotify Playlists',
        items: ['Driving Rock', 'Chill Hits', 'Road Trip', 'Morning Coffee', 'Night Drive'].map(byName),
      },
    ],
  }
}

/** Home's chips, as Spotify's own: each a facet of Home. */
const CHIPS = [
  { id: 'music', label: 'Music' },
  { id: 'podcasts', label: 'Podcasts' },
]

function home(facet: string): Answers['home'] {
  if (facet === 'podcasts') {
    return {
      chips: CHIPS,
      shortcuts: SHOWS.slice(0, 4),
      sections: [
        { title: 'Your shows', items: SHOWS },
        { title: 'Popular podcasts', items: [...SHOWS].reverse() },
      ],
    }
  }
  const music = facet === 'music'
  return {
    chips: CHIPS,
    shortcuts: [
      LIKED.item,
      byName('Daily Mix 1'),
      byName('Night Drive'),
      album('Fuel & Fire'),
      byName('Discover Weekly'),
      artistItem(ARTISTS[2]),
      byName('On Repeat'),
      byName('Long haul'),
    ],
    sections: [
      {
        title: 'Made For You',
        items: [
          'Daily Mix 1',
          'Daily Mix 2',
          'Daily Mix 3',
          'Daily Mix 4',
          'Discover Weekly',
          'Release Radar',
          'On Repeat',
        ].map(byName),
      },
      {
        title: 'Jump back in',
        items: [
          album('Blue Hour'),
          artistItem(ARTISTS[1]),
          byName('Road Trip'),
          album('Polar Night'),
          artistItem(ARTISTS[6]),
          album('Westbound'),
        ],
      },
      ...(music ? [] : [{ title: 'Your shows', items: SHOWS.slice(0, 4) }]),
      { title: 'Your top mixes', items: ['Rock Mix', 'Chill Mix', '2020s Mix', 'Dance Mix'].map(byName) },
      {
        title: 'Popular playlists',
        items: ['Driving Rock', 'Chill Hits', 'Road Trip', 'Morning Coffee', 'Night Drive'].map(byName),
      },
    ],
  }
}

function library(): SpotifyItem[] {
  return [
    LIKED.item,
    { ...byName('Long haul'), pinned: true, downloaded: true },
    byName('Daily Mix 1'),
    album('Fuel & Fire'),
    artistItem(ARTISTS[2]),
    byName('Night Drive'),
    SHOWS[0],
    album('Blue Hour'),
    artistItem(ARTISTS[0]),
    byName('Discover Weekly'),
    artistItem(ARTISTS[1]),
    album('Golden Exit'),
    byName('Driving Rock'),
    SHOWS[2],
    artistItem(ARTISTS[4]),
    album('Red Eye'),
  ]
}

const CATEGORIES: [string, string, string | null][] = [
  ['Music', '#dc148c', 'Daily Mix 2'],
  ['Podcasts', '#006450', null],
  ['Made For You', '#1e3264', 'Discover Weekly'],
  ['New Releases', '#e8115b', 'Release Radar'],
  ['Driving', '#509bf5', 'Night Drive'],
  ['Rock', '#e61e32', 'Driving Rock'],
  ['Pop', '#148a08', 'Daily Mix 4'],
  ['Chill', '#d84000', 'Chill Hits'],
  ['Hip-Hop', '#ba5d07', 'Daily Mix 2'],
  ['Dance/Electronic', '#8d67ab', 'Dance Mix'],
  ['Focus', '#503750', 'Daily Mix 3'],
  ['Sleep', '#1e3264', 'Chill Mix'],
]

function browse(): SpotifyCategory[] {
  return CATEGORIES.map(([name, color, cover]) => ({ name, color, cover: cover ? byName(cover) : SHOWS[1] }))
}

function search(query: string): SpotifyResults {
  const wanted = slug(query)
  const matches = (name: string) => slug(name).includes(wanted)
  const tagged = (artist: Artist) => artist.tags.some((tag) => slug(tag) === wanted || wanted.startsWith(slug(tag)))
  const artists = ARTISTS.filter((a) => matches(a.name) || tagged(a))
  const tracks = [
    ...ALL_TRACKS.filter((t) => matches(t.name)),
    ...ALL_TRACKS.filter((t) => !matches(t.name) && artists.some((a) => t.artists[0].uri === artistUri(a))),
  ].filter((track, i, all) => all.findIndex((t) => t.name === track.name) === i)
  const playlists = PLAYLISTS.filter((p) => matches(p.item.name) || matches(p.item.description)).map((p) => p.item)
  const albums = ALBUMS.filter((a) => matches(a.item.name) || artists.includes(a.artist)).map((a) => a.item)
  // Browse's Podcasts (or a search for them): every show.
  const shows = wanted.startsWith('podcast') ? SHOWS : SHOWS.filter((s) => matches(s.name))
  const topArtist = artists.find((a) => matches(a.name))
  const top = topArtist
    ? artistItem(topArtist)
    : (playlists[0] ??
      albums.find((a) => matches(a.name)) ??
      shows[0] ??
      (tracks[0] ? null : artists[0] ? artistItem(artists[0]) : null))
  return {
    top,
    topTrack: top ? null : (tracks[0] ?? null),
    tracks: tracks.slice(0, 8),
    artists: artists.map(artistItem),
    playlists: [...playlists, ...shows],
    albums,
  }
}

function collection(uri: string): SpotifyCollection {
  if (uri === LIKED.item.uri) return { item: LIKED.item, description: '', owner: '', year: null, tracks: LIKED.tracks }
  const found = ALBUMS.find((a) => a.item.uri === uri)
  if (found)
    return { item: found.item, description: '', owner: found.artist.name, year: found.year, tracks: found.tracks }
  const list = [...PLAYLISTS, ...CHARTS, ...RADIOS].find((p) => p.item.uri === uri)
  if (list)
    return { item: list.item, description: list.item.description, owner: list.item.by, year: null, tracks: list.tracks }
  const show = SHOWS.find((s) => s.uri === uri) ?? SHOWS[0]
  // A podcast's episodes are songs here: the preview has no episodes of its own.
  return { item: show, description: show.description, owner: show.by, year: null, tracks: mix(show.name, 12) }
}

function artist(uri: string): SpotifyArtist {
  const found = ARTISTS.find((a) => artistUri(a) === uri) ?? ARTISTS[0]
  const own = ALBUMS.filter((a) => a.artist === found)
  return {
    item: artistItem(found),
    listeners: 180_000 + ((found.name.length * 731_117) % 4_800_000),
    verified: true,
    popular: own
      .flatMap((a) => a.tracks)
      .filter((_, i) => i % 3 === 0)
      .slice(0, 5),
    releases: own.map((a) => a.item),
  }
}

/** The preview's answer, a moment later (as an answer from Spotify would be). */
export function previewAnswer(question: Question): Promise<unknown> {
  const answer = (() => {
    switch (question.ask) {
      case 'home':
        return question.signedIn ? home(question.facet) : publicHome()
      case 'library':
        return library()
      case 'browse':
        return browse()
      case 'search':
        return search(question.query)
      case 'collection':
        return collection(question.uri)
      case 'artist':
        return artist(question.uri)
      case 'lyrics':
        return previewLyrics({ background: '#5b7c99', text: '#000000', highlight: '#ffffff' }, 'WestPlay')
    }
  })()
  return new Promise((resolve) => setTimeout(() => resolve(answer), 120))
}
