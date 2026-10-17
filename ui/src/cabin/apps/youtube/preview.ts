import type { YouTubeAnswer, YouTubeChannel, YouTubePlaylist, YouTubeVideo } from '../../../shared/host'
import type { Question } from './youtube'

/**
 * DEVELOPMENT PREVIEW: in a plain browser there's no host to ask YouTube, so invented videos answer (no
 * thumbnails: the app draws its own placeholder). Never used inside WestPlay.
 */

const CHANNELS = ['Nordic Haul', 'Road Diaries', 'Sim Roads', 'Bus Workshop', 'Convoy Club']
const TITLES = [
  "Norway's coast road, start to finish: Bergen to Ålesund",
  'Night run Kiel to Hamburg, no music, just the road',
  'Coaches DLC: how tourist jobs work',
  'Every coach mod worth installing in 2026, tested on a long route',
  'Winter tyres or chains? A Finnish driver explains',
  "Crossing Spain's interior in one day",
  'Long drive mix: three hours of calm',
  'Scenic tour preview, Scandinavia by coach',
  "Let's play, episode 92",
  '1.61 update changelog',
]

function video(index: number, query: string): YouTubeVideo {
  const live = index % 7 === 3
  return {
    // Not real ids: 11 characters, as YouTube's.
    video: `preview${String(index).padStart(4, '0')}`,
    title: index === 0 && query ? `${TITLES[0]} (${query})` : TITLES[index % TITLES.length],
    channel: CHANNELS[index % CHANNELS.length],
    channelId: `UCpreview${String(index % CHANNELS.length).padStart(15, '0')}`,
    avatar: null,
    thumbnail: null,
    duration: live ? '' : `${3 + ((index * 7) % 50)}:${String((index * 13) % 60).padStart(2, '0')}`,
    views: `${((index * 37) % 900) + 12}K`,
    age: `${(index % 6) + 1} days ago`,
    live,
  }
}

/** An invented channel (its id: "UC" and 22 characters, as YouTube's). */
function channel(index: number, name: string): YouTubeChannel {
  return {
    channel: `UCpreview${String(index).padStart(15, '0')}`,
    name,
    avatar: null,
    banner: null,
    handle: `@${name.replace(/\s/g, '')}`,
    subscribers: `${((index * 53) % 900) + 40}K subscribers`,
    videos: `${((index * 17) % 300) + 20} videos`,
    subscribed: false,
  }
}

/** An invented playlist (its id: "PL" and 32 characters, as YouTube's). */
function playlist(index: number, after: number): YouTubePlaylist {
  return {
    playlist: `PLpreview${String(index).padStart(25, '0')}`,
    title: ['Long drive mix', 'Every Scandinavia video', 'Night runs, start to finish', 'Coach tours'][index % 4],
    channel: CHANNELS[index % CHANNELS.length],
    channelId: `UCpreview${String(index % CHANNELS.length).padStart(15, '0')}`,
    thumbnail: null,
    count: `${((index * 11) % 90) + 8} videos`,
    views: `${((index * 29) % 900) + 3}K views`,
    after,
  }
}

const SUGGESTIONS = ['coaches dlc', 'coaches tourist jobs', 'coach mod scania', 'scania', 'scania sound mod']

export function previewAnswer(question: Question): Promise<YouTubeAnswer> {
  let answer: YouTubeAnswer
  switch (question.action) {
    case 'search': {
      const from = question.more ? Number(question.more) : 0
      const { filter } = question
      const found =
        question.more || filter === 'playlists'
          ? []
          : CHANNELS.slice(0, filter ? 5 : 1).map((name, i) => channel(i, name))
      answer = {
        videos: filter
          ? []
          : Array.from({ length: 12 }, (_, i) => video(from + i, question.more ? '' : question.query)),
        channels: found,
        playlists:
          filter === 'playlists'
            ? Array.from({ length: 8 }, (_, i) => playlist(i, 0))
            : filter
              ? []
              : [playlist(from, 2), playlist(from + 1, 7)],
        more: !filter && from < 36 ? String(from + 12) : null,
      }
      break
    }
    case 'playlist': {
      const from = question.more ? Number(question.more) : 400
      answer = {
        playlist: question.more ? null : playlist(Number(question.playlist.slice(-2)) || 0, 0),
        videos: Array.from({ length: 12 }, (_, i) => video(from + i, '')),
        more: from < 424 ? String(from + 12) : null,
      }
      break
    }
    case 'channel': {
      const from = question.more ? Number(question.more) : 300
      answer = {
        channel: question.more ? null : channel(Number(question.channel.slice(-2)) || 0, CHANNELS[0]),
        videos: Array.from({ length: 12 }, (_, i) => video(from + i, '')),
        more: String(from + 12),
      }
      break
    }
    case 'suggest':
      answer = { suggestions: SUGGESTIONS.map((s) => `${question.query.trim()} ${s}`.trim()) }
      break
    case 'related':
      answer = { videos: Array.from({ length: 10 }, (_, i) => video(i + 50, '')) }
      break
    case 'subscribe':
      answer = {}
      break
    case 'feed': {
      const from = question.more ? Number(question.more) : question.which === 'home' ? 100 : 200
      answer = { videos: Array.from({ length: 12 }, (_, i) => video(from + i, '')), more: String(from + 12) }
      break
    }
  }
  return new Promise((resolve) => setTimeout(() => resolve(answer), 350))
}
