import { type Language, languageOf, type Strings, stringsFor } from './strings'
import { useLive } from './store'

export type { Language, Strings } from './strings'

/** The cabin's language: the one chosen in the companion, or the game's. */
export function useLanguage(): Language {
  return languageOf(useLive((state) => state.chosenLanguage ?? state.gameLanguage))
}

/** The cabin's words, in its language. */
export function useText(): Strings {
  return stringsFor(useLanguage())
}

/** Google's own tags where they aren't a locale's language alone (YouTube's and YouTube Music's `hl`). */
const GOOGLE_LANGUAGES: Record<string, string> = {
  'en-GB': 'en-GB',
  'es-419': 'es-419',
  'fr-CA': 'fr-CA',
  'pt-PT': 'pt-PT',
  'zh-Hans-CN': 'zh-CN',
  'zh-Hant-TW': 'zh-TW',
  'sr-Latn-RS': 'sr-Latn',
  'nb-NO': 'no',
}

/** The language YouTube and YouTube Music are asked to answer in, for the cabin's locale (`de-DE`: `de`,
 * `pt-PT`: `pt-PT`, `zh-Hant-TW`: `zh-TW`). Apple Music is asked in the locale itself. */
export function googleLanguage(locale: string): string {
  return GOOGLE_LANGUAGES[locale] ?? locale.split('-')[0]
}
