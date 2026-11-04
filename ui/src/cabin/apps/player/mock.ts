import type { PlayerSong } from '../../../shared/host'

/**
 * Made-up songs for Player in a plain browser (no host to list the game's music folder: the cab's preview).
 * Every name here is invented; two have no tags, as songs in the folder may not.
 */
export const SONGS: PlayerSong[] = [
  { file: 'Long Road North.mp3', title: 'Long Road North', artist: 'The Milemarkers', seconds: 240 },
  { file: 'Diesel and Rain.mp3', title: 'Diesel and Rain', artist: 'The Milemarkers', seconds: 228 },
  { file: 'Border Lights.mp3', title: 'Border Lights', artist: 'The Milemarkers', seconds: 262 },
  { file: 'Night Drive.ogg', title: 'Night Drive', artist: 'Kaleo Sands', seconds: 252 },
  { file: 'Harbour at Two.ogg', title: 'Harbour at Two', artist: 'Kaleo Sands', seconds: 301 },
  { file: 'track07_final.mp3', seconds: 301 },
  { file: 'Windscreen.mp3', title: 'Windscreen', artist: 'Grey Lantern', seconds: 214 },
  { file: 'Mirage.mp3', title: 'Mirage', artist: 'Sol Avenue', seconds: 276 },
  { file: 'Timberline.mp3', title: 'Timberline', artist: 'North Pines', seconds: 245 },
  { file: 'weigh station (live).mp3', seconds: 208 },
]
