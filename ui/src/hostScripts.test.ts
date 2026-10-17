import { describe, expect, it } from 'vitest'
import appleMusic from '../../crates/host/src/apple_music.js?raw'
import appleMusicSignIn from '../../crates/host/src/apple_music_sign_in.js?raw'
import cabSound from '../../crates/host/src/cab_sound.js?raw'
import spotify from '../../crates/host/src/spotify.js?raw'
import youtube from '../../crates/host/src/youtube.js?raw'
import youtubeMusic from '../../crates/host/src/youtube_music.js?raw'

/**
 * The scripts the host runs in the players' pages (crates/host/src), each as the host puts it in: the truck
 * speakers' script first, then the player's own, one script. One that doesn't compile does nothing at all (the
 * player never answers), so each is compiled here, never run.
 */
const AS_RUN: Record<string, string[]> = {
  YouTube: [cabSound, youtube],
  'Apple Music': [cabSound, appleMusic],
  'YouTube Music': [cabSound, youtubeMusic],
  Spotify: [cabSound, spotify],
  "Apple Music's sign-in": [appleMusicSignIn],
}

describe("the players' page scripts", () => {
  for (const [player, parts] of Object.entries(AS_RUN)) {
    it(`compile for ${player}`, () => {
      expect(() => new Function(parts.join('\n'))).not.toThrow()
    })
  }
})
