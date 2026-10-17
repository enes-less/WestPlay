import type {
  YouTubeMusicAnswers,
  YouTubeMusicCollection,
  YouTubeMusicItem,
  YouTubeMusicPage,
  YouTubeMusicQuestion,
  YouTubeMusicSection,
  YouTubeMusicWatch,
} from '../../../shared/host'

/**
 * YouTube Music's preview, outside WestPlay (a plain browser): an invented catalogue (made-up artists, albums,
 * playlists and songs, no pictures: the app draws their covers) answering every question as YouTube Music
 * would, after a moment, and playing it here (youtubeMusic.ts).
 */

const ARTISTS = ['Diesel Hearts', 'Mira Vale', 'Nordkapp', 'Kaya Sol', 'Road Diaries', 'Asfalt Kuşları']
const SONG_NAMES = [
  'Night Drive',
  'Weigh Station',
  'Paper Maps',
  'Golden Exit',
  'Coastline',
  'Blue Hour',
  'Diesel Rock',
  'Open Country',
  'Fuel & Fire',
  'Kilometre Zero',
  'Gece Yolu',
  'Last Ferry',
  'Rain on the Glass',
  'Low Gear',
  'Border Lights',
  'Morning Coffee',
]

function song(index: number, list?: string, at?: number): YouTubeMusicItem {
  const artist = ARTISTS[index % ARTISTS.length]
  const video = `preview${String(index).padStart(4, '0')}`
  const minutes = 2 + (index % 3)
  const seconds = (index * 17) % 60
  return {
    kind: index % 5 === 4 ? 'video' : 'song',
    title: SONG_NAMES[index % SONG_NAMES.length],
    subtitle: `${artist} • ${index % 2 ? 'Long Haul' : 'Kilometre Zero'}`,
    video,
    play: { video, ...(list ? { playlist: list, index: at ?? 0 } : { playlist: `RDAMVM${video}` }) },
    artist: { id: `UCpreview${index % ARTISTS.length}` },
    album: { id: `MPREbpreview${index % 4}` },
    duration: `${minutes}:${String(seconds).padStart(2, '0')}`,
    explicit: index % 7 === 3,
  }
}

const songs = (from: number, count: number, list?: string) =>
  Array.from({ length: count }, (_, i) => song(from + i, list, i))

function album(index: number): YouTubeMusicItem {
  return {
    kind: 'album',
    title: ['Kilometre Zero', 'Long Haul', 'Night Shift', 'Bosphorus Bridge', 'Tachograph', 'Rest Area'][index % 6],
    subtitle: `Album • ${ARTISTS[index % ARTISTS.length]}`,
    browse: { id: `MPREbpreview${index}` },
    play: { playlist: `OLAKpreview${index}` },
  }
}

function playlist(index: number): YouTubeMusicItem {
  return {
    kind: 'playlist',
    title: ['Truckers’ Mix', 'Long Night Drive', 'Turkish Road Pop', 'Cab Classics', 'Rainy Highway', 'Focus Lane'][
      index % 6
    ],
    subtitle: `Playlist • YouTube Music • ${20 + index * 7} songs`,
    browse: { id: `VLPLpreview${index}` },
    play: { playlist: `PLpreview${index}` },
  }
}

function artist(index: number): YouTubeMusicItem {
  return {
    kind: 'artist',
    title: ARTISTS[index % ARTISTS.length],
    subtitle: `Artist • ${(index + 1) * 1.3}M monthly audience`,
    browse: { id: `UCpreview${index % ARTISTS.length}` },
  }
}

const MOOD_COLORS = ['#c76df2', '#4fb0ff', '#ffb000', '#ff4e45', '#3ad48c', '#ff7ab8', '#7e8cff', '#00c2c7']
const MOODS = ['Chill', 'Commute', 'Energize', 'Feel good', 'Focus', 'Party', 'Romance', 'Sleep', 'Workout', 'Sad']
const moods = (count: number): YouTubeMusicItem[] =>
  MOODS.slice(0, count).map((title, i) => ({
    kind: 'mood',
    title,
    subtitle: '',
    browse: { id: 'FEmusic_moods_and_genres_category', params: `preview${i}` },
    color: MOOD_COLORS[i % MOOD_COLORS.length],
  }))

const shelf = (title: string, items: YouTubeMusicItem[], strapline?: string): YouTubeMusicSection => ({
  title,
  ...(strapline ? { strapline } : {}),
  style: 'shelf',
  items,
})

function home(chip?: string): YouTubeMusicPage {
  const offset = chip ? Number(chip.replace(/\D/g, '')) * 3 : 0
  return {
    chips: MOODS.map((label, i) => ({ label, params: `chip${i}`, selected: chip === `chip${i}` })),
    sections: [
      { title: 'Quick picks', strapline: 'Start radio from a song', style: 'columns', items: songs(offset, 16) },
      shelf(
        'Listen again',
        [...songs(offset + 3, 2), album(1), artist(2), playlist(0), album(3)].map((each) => each),
      ),
      shelf(
        'Mixed for you',
        [0, 1, 2, 3, 4].map((i) => playlist(i + offset)),
      ),
      shelf(
        'Recommended albums',
        [0, 1, 2, 3, 4, 5].map((i) => album(i + offset)),
      ),
      shelf('Similar to Diesel Hearts', [1, 2, 3, 4].map(artist), 'More like this'),
    ],
    more: 'previewmore',
  }
}

function explore(): YouTubeMusicPage {
  return {
    chips: [],
    sections: [
      {
        title: '',
        style: 'moods',
        items: [
          { kind: 'mood', title: 'New releases', subtitle: '', browse: { id: 'FEmusic_new_releases' } },
          { kind: 'mood', title: 'Charts', subtitle: '', browse: { id: 'FEmusic_charts' } },
          { kind: 'mood', title: 'Moods & genres', subtitle: '', browse: { id: 'FEmusic_moods_and_genres' } },
        ],
      },
      shelf('New albums & singles', [0, 1, 2, 3, 4, 5].map(album)),
      { title: 'Moods & genres', style: 'moodRows', items: moods(10) },
      { title: 'Trending', style: 'columns', items: songs(5, 12).map((s, i) => ({ ...s, rank: String(i + 1) })) },
      shelf(
        'New music videos',
        songs(9, 6).map((s) => ({ ...s, kind: 'video' as const })),
      ),
    ],
  }
}

function collection(id: string): YouTubeMusicCollection {
  const index = Number(id.replace(/\D/g, '')) || 0
  const isAlbum = id.startsWith('MPRE')
  const card = isAlbum ? album(index) : playlist(index)
  const list = isAlbum ? `OLAKpreview${index}` : `PLpreview${index}`
  const tracks = songs(index * 2, isAlbum ? 10 : 24, list).map((s, i) => ({
    ...s,
    ...(isAlbum ? { rank: String(i + 1) } : {}),
  }))
  return {
    kind: isAlbum ? 'album' : 'playlist',
    title: card.title,
    subtitle: isAlbum ? 'Album • 2025' : 'Playlist • 2026',
    second: `${tracks.length} songs • ${Math.round(tracks.length * 3.4)} minutes`,
    by: isAlbum ? ARTISTS[index % ARTISTS.length] : 'YouTube Music',
    ...(isAlbum ? { byPage: { id: `UCpreview${index % ARTISTS.length}` } } : {}),
    description: isAlbum ? '' : 'Songs for the long road, picked for the cab.',
    play: { playlist: list },
    shuffle: { playlist: list, params: 'shuffle' },
    radio: { playlist: `RDAMPL${list}` },
    playlist: list,
    saved: false,
    tracks,
    sections: isAlbum ? [shelf(`More by ${ARTISTS[index % ARTISTS.length]}`, [1, 2, 3, 4].map(album))] : [],
  }
}

function answer(question: YouTubeMusicQuestion): YouTubeMusicAnswers[YouTubeMusicQuestion['ask']] {
  switch (question.ask) {
    case 'home':
      return question.more
        ? { chips: [], sections: [shelf('Turkish hits', [3, 4, 5].map(playlist))] }
        : home(question.chip)
    case 'explore':
      return explore()
    case 'browse':
      return {
        title: question.page.id === 'FEmusic_charts' ? 'Charts' : 'Mood',
        chips: [],
        sections: [
          shelf('Featured playlists', [0, 1, 2, 3].map(playlist)),
          { title: 'Top songs', style: 'songs', items: songs(2, 8) },
          { title: 'Moods & genres', style: 'moods', items: moods(10) },
        ],
      }
    case 'search': {
      const found = [artist(0), ...songs(0, 6), album(0), album(2), playlist(1), artist(3), ...songs(8, 4)]
      return {
        top: artist(0),
        topSongs: songs(0, 3),
        chips: ['Songs', 'Videos', 'Albums', 'Artists', 'Community playlists', 'Podcasts'].map((label, i) => ({
          label,
          params: `filter${i}`,
          selected: question.filter === `filter${i}`,
        })),
        items: question.filter ? songs(3, 12) : found,
      }
    }
    case 'suggest':
      return {
        texts: [question.query, `${question.query} mix`, `${question.query} live`].filter((t) => t.trim()),
        items: question.query.trim() ? [artist(0), song(2)] : [],
      }
    case 'collection':
      return collection(question.id)
    case 'artist': {
      const index = Number(question.id.replace(/\D/g, '')) || 0
      return {
        name: ARTISTS[index % ARTISTS.length],
        description: 'A band from the long roads, singing of night shifts, rest areas and the lights of the next town.',
        subscribers: '1.2M',
        shuffle: { playlist: `RDAOpreview${index}`, params: 'shuffle' },
        radio: { playlist: `RDEMpreview${index}` },
        sections: [
          { title: 'Top songs', style: 'songs', items: songs(index, 5) },
          shelf('Albums', [0, 1, 2].map(album)),
          shelf('Singles & EPs', [3, 4, 5].map(album)),
          shelf('Fans might also like', [1, 2, 4, 5].map(artist)),
          {
            title: 'About',
            style: 'about',
            items: [],
            text: 'Formed at a truck stop on the E80, they have played every rest area from Edirne to Kapıkule.',
          },
        ],
      }
    }
    case 'library':
      return {
        chips: [],
        sections: [
          question.shelf === 'songs'
            ? { title: '', style: 'songs', items: songs(4, 14) }
            : shelf(
                '',
                question.shelf === 'artists' || question.shelf === 'subscriptions'
                  ? [0, 1, 2, 3].map(artist)
                  : [0, 1, 2, 3, 4].map(playlist),
              ),
        ],
      }
    case 'lyrics':
      return {
        text: 'Headlights on the wet road\nThe radio low, the night is long\nOne more town before the dawn\n\nKilometre zero, kilometre zero\nWe start again where the road goes on',
        source: 'Source: Preview',
      }
    case 'related':
      return {
        chips: [],
        sections: [
          { title: 'You might also like', style: 'columns', items: songs(6, 8) },
          shelf('Recommended playlists', [2, 3, 4].map(playlist)),
          shelf('Similar artists', [1, 2, 3].map(artist)),
          { title: 'About the artist', style: 'about', items: [], text: 'Invented for the preview.' },
        ],
      }
  }
}

/** The preview's answer, after a moment (as YouTube Music's would come). */
export function previewAnswer(question: YouTubeMusicQuestion): Promise<unknown> {
  return new Promise((resolve) => setTimeout(() => resolve(answer(question)), 350))
}

/** What the preview plays for `watch`: a list's songs and the one to start with, and what it plays from. */
export function previewPlay(
  watch: YouTubeMusicWatch,
): { items: YouTubeMusicItem[]; index: number; from: string } | null {
  const list = watch.playlist ?? ''
  if (list.startsWith('OLAK') || list.startsWith('PL')) {
    const found = collection(list.startsWith('OLAK') ? `MPREb${list}` : `VL${list}`)
    const at = watch.video
      ? Math.max(
          0,
          found.tracks.findIndex((t) => t.video === watch.video),
        )
      : 0
    const items = watch.params === 'shuffle' ? [...found.tracks].sort(() => Math.random() - 0.5) : found.tracks
    return { items, index: watch.params === 'shuffle' ? 0 : at, from: found.title }
  }
  // A song on its own, or a radio: the song, then others like it.
  const first = Number(watch.video?.replace(/\D/g, '')) || 0
  const items = songs(first, 20)
  return { items, index: 0, from: `${items[0].title} radio` }
}
