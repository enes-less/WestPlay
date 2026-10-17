import type { CompanionWords } from '.'
import { latin } from '../../onboarding/words/sr_sp'
import { words as cyrillic } from './sr_sr'

/** Serbian, in Latin letters (the game's `sr_sp`): Serbian Cyrillic's words, letter for letter. */
export const words: CompanionWords = latin(cyrillic)
