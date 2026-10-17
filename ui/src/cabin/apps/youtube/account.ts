/** Google's image hosts, where an account's picture is (as the host lets it through: youtube.rs). */
const PICTURE_HOSTS = [
  'https://yt3.ggpht.com/',
  'https://yt3.googleusercontent.com/',
  'https://lh3.googleusercontent.com/',
]

/** YouTube's account as the cab shows it: signed in or not, and its picture (shared by YouTube Music). */
export interface Account {
  signedIn: boolean
  avatar: string | null
}

/**
 * The account as it was kept (shown at once as WestPlay starts, replaced as YouTube's page says it): signed in or out, and a picture only from Google's image hosts; anything else, nothing.
 */
export function keptAccount(kept: unknown): Account | null {
  if (!kept || typeof kept !== 'object') return null
  const { signedIn, avatar } = kept as { signedIn?: unknown; avatar?: unknown }
  if (typeof signedIn !== 'boolean') return null
  const picture =
    signedIn && typeof avatar === 'string' && PICTURE_HOSTS.some((host) => avatar.startsWith(host)) ? avatar : null
  return { signedIn, avatar: picture }
}
