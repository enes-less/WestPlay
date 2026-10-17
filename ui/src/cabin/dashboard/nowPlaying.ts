/** What the Dashboard's Now Playing can show: YouTube's video, the Spotify app, Apple Music (the Music app), or
 * YouTube Music. In this order when two play at once. */
export const NOW_PLAYING_ORDER = ['video', 'spotify', 'appleMusic', 'youtubeMusic'] as const

export type NowPlayingSource = (typeof NOW_PLAYING_ORDER)[number]

/**
 * What Now Playing shows: what plays (in `NOW_PLAYING_ORDER` when more than one does); nothing playing, what
 * played last while it still has something (a song paused, or between two songs, stays shown rather than a
 * paused video taking its place for the moment); else the first that has something; else nothing (`null`: Not
 * Playing).
 */
export function shownSource(
  playing: Record<NowPlayingSource, boolean>,
  has: Record<NowPlayingSource, boolean>,
  last: NowPlayingSource | null,
): NowPlayingSource | null {
  return (
    NOW_PLAYING_ORDER.find((each) => playing[each]) ??
    (last !== null && has[last] ? last : undefined) ??
    NOW_PLAYING_ORDER.find((each) => has[each]) ??
    null
  )
}
