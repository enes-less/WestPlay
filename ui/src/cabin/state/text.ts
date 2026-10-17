import { type Language, languageOf, STRINGS, type Strings } from './strings'
import { useLive } from './store'

export type { Language, Strings } from './strings'

/** The cabin's language: the game's. */
export function useLanguage(): Language {
  return languageOf(useLive((state) => state.gameLanguage))
}

/** The cabin's words, in the game's language. */
export function useText(): Strings {
  return STRINGS[languageOf(useLive((state) => state.gameLanguage))]
}
