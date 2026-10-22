import type { CompanionWords } from '.'
import { words as polish } from './pl_pl'

/** Silesian: Polish's words for now, as the onboarding's, with the game's own Silesian name for its Mod
 * Manager, so it matches what the player sees there. */
export const words: CompanionWords = {
  ...polish,
  uninstall: {
    ...polish.uninstall,
    profilesFailed:
      'Nie udało się zmienić profili ({n}) i nadal zawierają WestPlay. Usuń go tam w menu „Mynedżer modōw”.',
  },
}
