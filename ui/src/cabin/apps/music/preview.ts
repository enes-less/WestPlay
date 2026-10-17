import { previewLyrics } from '../lyricsTime'
import type {
  AppleMusicArtist,
  AppleMusicCollection,
  AppleMusicItem,
  AppleMusicLibrary,
  AppleMusicPage,
  AppleMusicResults,
  AppleMusicSection,
  AppleMusicTrack,
  Answers,
  Question,
} from './appleMusic'

/**
 * DEVELOPMENT PREVIEW: in a plain browser (no host, no web player), invented artists, albums, playlists and
 * stations answer the app (no pictures: the app draws covers in their colours). Nothing here is anyone's real
 * music.
 */

interface Artist {
  key: string
  name: string
  color: string
  albums: { key: string; name: string; year: number; color: string; genre: string; songs: string[] }[]
}

const ARTISTS: Artist[] = [
  {
    key: '1',
    name: 'Mira Vale',
    color: '#c2477a',
    albums: [
      {
        key: '11',
        name: 'Blue Hour',
        year: 2026,
        color: '#3b5fc0',
        genre: 'Pop',
        songs: ['Blue Hour', 'Glass Coast', 'Mile Marker', 'Low Sun', 'Night Ferry', 'Ninety Days', 'Afterglow'],
      },
      {
        key: '12',
        name: 'Paper Maps',
        year: 2023,
        color: '#d4875b',
        genre: 'Pop',
        songs: ['Paper Maps', 'Two Lanes', 'Postcards', 'Static', 'Home by Ten'],
      },
    ],
  },
  {
    key: '2',
    name: 'Diesel Hearts',
    color: '#b5432e',
    albums: [
      {
        key: '21',
        name: 'Fuel & Fire',
        year: 2025,
        color: '#c8361f',
        genre: 'Rock',
        songs: ['Fuel & Fire', 'Jackknife', 'Engine Brake', 'Red Line', 'Long Haul', 'Black Ice'],
      },
      {
        key: '22',
        name: 'Weigh Station',
        year: 2021,
        color: '#6b6f2a',
        genre: 'Rock',
        songs: ['Weigh Station', 'Overload', 'Detour', 'Truck Stop Blues'],
      },
    ],
  },
  {
    key: '3',
    name: 'Nordkapp',
    color: '#2f6f86',
    albums: [
      {
        key: '31',
        name: 'Polar Night',
        year: 2026,
        color: '#1f4e8c',
        genre: 'Electronic',
        songs: ['Polar Night', 'Aurora', 'Fjord', 'Tundra Line', 'Midnight Sun', 'Snow Chains', 'Drift'],
      },
      {
        key: '32',
        name: 'Coastline',
        year: 2023,
        color: '#2a8c7d',
        genre: 'Electronic',
        songs: ['Coastline', 'Lighthouse', 'Undertow', 'Saltwater'],
      },
    ],
  },
  {
    key: '4',
    name: 'Kaya Sol',
    color: '#d9a13b',
    albums: [
      {
        key: '41',
        name: 'Golden Exit',
        year: 2025,
        color: '#e0a526',
        genre: 'Dance',
        songs: ['Golden Exit', 'Mirrorball', 'Heatwave', 'Slow Dance', 'Overtime'],
      },
    ],
  },
  {
    key: '5',
    name: 'The Long Roads',
    color: '#8a5a3c',
    albums: [
      {
        key: '51',
        name: 'Westbound',
        year: 2024,
        color: '#a8622d',
        genre: 'Country',
        songs: ['Westbound', 'Dust & Diesel', 'Prairie Radio', 'Last Exit', 'Wide Open', 'Ghost Town'],
      },
    ],
  },
]

const DURATIONS = [187, 214, 242, 199, 226, 258, 173, 205]

function albumItem(artist: Artist, album: Artist['albums'][number]): AppleMusicItem {
  return {
    id: `albums:${album.key}`,
    kind: 'album',
    name: album.name,
    by: artist.name,
    note: '',
    image: null,
    wide: null,
    color: album.color,
    explicit: false,
    live: false,
  }
}

function artistItem(artist: Artist): AppleMusicItem {
  return {
    id: `artists:${artist.key}`,
    kind: 'artist',
    name: artist.name,
    by: '',
    note: '',
    image: null,
    wide: null,
    color: artist.color,
    explicit: false,
    live: false,
  }
}

function songsOf(artist: Artist, album: Artist['albums'][number]): AppleMusicTrack[] {
  return album.songs.map((name, i) => ({
    id: `songs:${album.key}${i}`,
    name,
    artist: artist.name,
    album: album.name,
    albumId: `albums:${album.key}`,
    artistId: `artists:${artist.key}`,
    image: null,
    color: album.color,
    duration: DURATIONS[(i + Number(album.key)) % DURATIONS.length],
    explicit: i === 2,
    number: i + 1,
  }))
}

const ALBUMS = ARTISTS.flatMap((artist) => artist.albums.map((album) => ({ artist, album })))
const ALL_SONGS = ALBUMS.flatMap(({ artist, album }) => songsOf(artist, album))

function songItem(track: AppleMusicTrack): AppleMusicItem {
  return {
    id: track.id,
    kind: 'song',
    name: track.name,
    by: track.artist,
    note: '',
    image: null,
    wide: null,
    color: track.color,
    explicit: track.explicit,
    live: false,
  }
}

interface Playlist {
  key: string
  name: string
  by: string
  color: string
  note: string
  songs: number[]
}

const PLAYLISTS: Playlist[] = [
  {
    key: 'pl.1',
    name: 'Today’s Road',
    by: 'Apple Music',
    color: '#d63a52',
    note: 'The biggest songs on the move.',
    songs: [0, 7, 12, 19, 25, 1, 31, 14],
  },
  {
    key: 'pl.2',
    name: 'Night Drive',
    by: 'Apple Music Electronic',
    color: '#28307a',
    note: 'Lights off, eyes on the road.',
    songs: [19, 20, 21, 22, 26, 2, 5],
  },
  {
    key: 'pl.3',
    name: 'Diesel Rock',
    by: 'Apple Music Rock',
    color: '#7a2a1c',
    note: 'Guitars for the long haul.',
    songs: [12, 13, 14, 15, 16, 17, 18],
  },
  {
    key: 'pl.4',
    name: 'Open Country',
    by: 'Apple Music Country',
    color: '#9a6a2a',
    note: 'Wide skies and steel guitars.',
    songs: [35, 36, 37, 38, 39, 40],
  },
  {
    key: 'pl.5',
    name: 'Morning Coffee',
    by: 'Apple Music Chill',
    color: '#6a8a5a',
    note: 'An easy start before the first load.',
    songs: [7, 8, 9, 10, 4, 33],
  },
  { key: 'pl.6', name: 'Dance Hits', by: 'Apple Music Dance', color: '#b0309a', note: '', songs: [30, 31, 32, 33, 34] },
]

function playlistItem(playlist: Playlist): AppleMusicItem {
  return {
    id: `playlists:${playlist.key}`,
    kind: 'playlist',
    name: playlist.name,
    by: playlist.by,
    note: playlist.note,
    image: null,
    wide: null,
    color: playlist.color,
    explicit: false,
    live: false,
  }
}

const STATIONS: AppleMusicItem[] = [
  { name: 'WestPlay Radio 1', by: 'The new music that matters.', color: '#e0324a', live: true },
  { name: 'Highway Hits', by: 'Hits for the whole route.', color: '#2a6fd6', live: true },
  { name: 'Rock Station', by: '', color: '#8a2e1e', live: false },
  { name: 'Chill Station', by: '', color: '#3a8a7a', live: false },
  { name: 'Country Station', by: '', color: '#a8742d', live: false },
].map((station, i) => ({
  id: `stations:ra.${i}`,
  kind: 'station',
  name: station.name,
  by: station.by,
  note: '',
  image: null,
  wide: null,
  color: station.color,
  explicit: false,
  live: station.live,
}))

const CATEGORIES: AppleMusicItem[] = [
  ['Hits', '#d63a52'],
  ['Pop', '#e06aa8'],
  ['Rock', '#8a2e1e'],
  ['Country', '#a8742d'],
  ['Electronic', '#28307a'],
  ['Dance', '#b0309a'],
  ['Chill', '#3a8a7a'],
  ['Charts', '#2a6fd6'],
  ['Decades', '#6a4ab0'],
  ['Feel Good', '#e0a526'],
].map(([name, color], i) => ({
  id: `apple-curators:${i}`,
  kind: 'curator',
  name,
  by: '',
  note: '',
  image: null,
  wide: null,
  color,
  explicit: false,
  live: false,
}))

const albums = ALBUMS.map(({ artist, album }) => albumItem(artist, album))
const playlists = PLAYLISTS.map(playlistItem)

function shelf(title: string, items: AppleMusicItem[]): AppleMusicSection {
  return { title, style: 'shelf', items, songs: [] }
}

function hero(items: AppleMusicItem[], labels: string[]): AppleMusicSection {
  return {
    title: '',
    style: 'hero',
    items: items.map((item, i) => ({ ...item, label: labels[i % labels.length], note: item.note || item.by })),
    songs: [],
  }
}

const NEW: AppleMusicPage = {
  sections: [
    hero(
      [playlists[0], albums[0], playlists[1], albums[4]],
      ['UPDATED PLAYLIST', 'NEW ALBUM', 'UPDATED PLAYLIST', 'NEW ALBUM'],
    ),
    {
      title: 'Best New Songs',
      style: 'songs',
      items: [],
      songs: [0, 12, 19, 30, 35, 7, 13, 20].map((i) => ALL_SONGS[i]),
    },
    shelf('New Releases', albums),
    shelf('Updated Playlists', playlists),
  ],
}

const RADIO: AppleMusicPage = {
  sections: [hero(STATIONS.slice(0, 2), ['LIVE', 'LIVE']), shelf('Top Stations', STATIONS.slice(2))],
}

const HOME: AppleMusicPage = {
  sections: [
    { title: '', style: 'recent', items: [albums[2], playlists[1], albums[0], STATIONS[0], playlists[2]], songs: [] },
    shelf('Top Picks for You', [playlists[4], albums[5], playlists[3], albums[6], albums[1]]),
    shelf('Made for You', playlists.slice(0, 4)),
    shelf('Recently Played', [albums[3], albums[7], playlists[5]]),
  ],
}

function collection(id: string): AppleMusicCollection {
  const found = ALBUMS.find(({ album }) => `albums:${album.key}` === id)
  if (found) {
    return {
      item: albumItem(found.artist, found.album),
      description: '',
      genre: found.album.genre,
      year: found.album.year,
      tracks: songsOf(found.artist, found.album),
    }
  }
  const playlist = PLAYLISTS.find((each) => `playlists:${each.key}` === id)
  if (!playlist) throw new Error('not in the preview')
  return {
    item: playlistItem(playlist),
    description: playlist.note,
    genre: '',
    year: null,
    tracks: playlist.songs.map((i) => ALL_SONGS[i % ALL_SONGS.length]),
  }
}

function artist(id: string): AppleMusicArtist {
  const found = ARTISTS.find((each) => `artists:${each.key}` === id)
  if (!found) throw new Error('not in the preview')
  const own = found.albums.map((album) => albumItem(found, album))
  return {
    item: artistItem(found),
    top: found.albums.flatMap((album) => songsOf(found, album)).slice(0, 8),
    sections: [
      shelf('Latest Release', own.slice(0, 1)),
      shelf('Albums', own),
      shelf('Similar Artists', ARTISTS.filter((each) => each !== found).map(artistItem)),
    ],
  }
}

function search(query: string): AppleMusicResults {
  const wanted = query.toLowerCase()
  const has = (name: string) => name.toLowerCase().includes(wanted)
  const songs = ALL_SONGS.filter((song) => has(song.name) || has(song.artist))
  const artists = ARTISTS.filter((each) => has(each.name)).map(artistItem)
  const found = albums.filter((album) => has(album.name) || has(album.by))
  const lists = playlists.filter((playlist) => has(playlist.name))
  return {
    top: [...artists.slice(0, 1), ...found.slice(0, 2), ...songs.slice(0, 3).map(songItem)],
    songs,
    artists,
    albums: found,
    playlists: lists,
    stations: STATIONS.filter((station) => has(station.name)),
  }
}

/** The preview's Library sorts: as Apple Music names them (in English: the preview has no language). */
const SORTS = {
  albums: [
    { id: '-dateAdded', label: 'Recently Added' },
    { id: 'name', label: 'Title' },
    { id: 'artistName', label: 'Artist' },
  ],
  songs: [
    { id: '-dateAdded', label: 'Recently Added' },
    { id: 'name', label: 'Title' },
    { id: 'artistName', label: 'Artist' },
    { id: 'albumName', label: 'Album' },
  ],
  playlists: [
    { id: 'name', label: '' },
    { id: '-dateAdded', label: '' },
    { id: '-lastPlayed', label: '' },
    { id: '-updated', label: '' },
    { id: 'type', label: '' },
  ],
}

/** Every third one a favourite, in the preview. */
function favourite<T>(list: T[]): (T & { favorite: boolean })[] {
  return list.map((each, i) => ({ ...each, favorite: i % 3 === 1 }))
}

function sorted<T extends { name: string }>(list: T[], sort: string, by: (each: T) => string): T[] {
  if (sort === 'name') return [...list].sort((a, b) => a.name.localeCompare(b.name))
  if (sort === 'artistName' || sort === 'albumName') return [...list].sort((a, b) => by(a).localeCompare(by(b)))
  return list
}

function library(shelf: string, sort?: string): AppleMusicLibrary {
  if (shelf === 'songs') {
    const chosen = sort ?? '-dateAdded'
    const songs = sorted(favourite(ALL_SONGS.slice(0, 24)), chosen, (song) =>
      chosen === 'albumName' ? song.album : song.artist,
    )
    return { items: [], songs, sorts: SORTS.songs, sort: chosen }
  }
  if (shelf === 'artists') return { items: ARTISTS.map(artistItem), songs: [], sorts: [], sort: null }
  if (shelf === 'albums') {
    const chosen = sort ?? '-dateAdded'
    return {
      items: sorted(favourite(albums), chosen, (album) => album.by),
      songs: [],
      sorts: SORTS.albums,
      sort: chosen,
    }
  }
  if (shelf === 'playlists') {
    const chosen = sort ?? '-lastPlayed'
    return {
      items: sorted(favourite(playlists), chosen, (playlist) => playlist.by),
      songs: [],
      sorts: SORTS.playlists,
      sort: chosen,
    }
  }
  return {
    items: [albums[0], playlists[1], albums[3], albums[5], playlists[3], albums[2], albums[7]],
    songs: [],
    sorts: [],
    sort: null,
  }
}

function answer(question: Question): Answers[Question['ask']] {
  switch (question.ask) {
    case 'home':
      return HOME
    case 'new':
      return NEW
    case 'radio':
      return RADIO
    case 'browse':
      return CATEGORIES
    case 'curator':
      return {
        sections: [
          shelf('Playlists', playlists.slice(0, 4)),
          shelf('New Releases', albums.slice(2, 7)),
          shelf('Stations', STATIONS.slice(2)),
        ],
      }
    case 'search':
      return search(question.query)
    case 'collection':
      return collection(question.id)
    case 'artist':
      return artist(question.id)
    case 'library':
      return library(question.shelf, question.sort)
    case 'lyrics':
      return previewLyrics()
  }
}

/** The preview's answer, a moment later (as if asked of Apple Music). */
export function previewAnswer<Q extends Question>(question: Q): Promise<Answers[Q['ask']]> {
  return new Promise((resolve, reject) => {
    setTimeout(() => {
      try {
        resolve(answer(question) as Answers[Q['ask']])
      } catch (error) {
        reject(error)
      }
    }, 350)
  })
}

/** What a station or a song on its own plays in the preview: its album (a song's), or a station's mix. */
export function previewQueue(
  item: AppleMusicItem,
): { context: AppleMusicItem | null; tracks: AppleMusicTrack[]; index: number } | null {
  if (item.kind === 'station') {
    const start = Number(item.id.slice(-1)) * 5
    return { context: item, tracks: [...ALL_SONGS.slice(start), ...ALL_SONGS.slice(0, start)], index: 0 }
  }
  const song = ALL_SONGS.find((each) => each.id === item.id)
  if (!song?.albumId) return null
  const page = collection(song.albumId)
  return { context: page.item, tracks: page.tracks, index: page.tracks.findIndex((each) => each.id === song.id) }
}
