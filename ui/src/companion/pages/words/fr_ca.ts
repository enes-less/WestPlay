import type { CompanionWords } from '.'
import { words as france } from './fr_fr'

/** French (Canada): France's French, with Québec's usage where it differs ("plugiciel"). */
export const words: CompanionWords = {
  ...france,
  setup: {
    ...france.setup,
    plugin: 'Plugiciel',
    checking: { title: 'Analyse du jeu', text: 'Version, plugiciel et mods' },
  },
  uninstall: {
    ...france.uninstall,
    steps: { ...france.uninstall.steps, plugin: 'Suppression du plugiciel' },
  },
  about: { ...france.about, log: 'Journal du plugiciel' },
  update: {
    ...france.update,
    finish: {
      ...france.update.finish,
      text: 'Le plugiciel du nouveau WestPlay va être installé dans le jeu. Windows demandera l’autorisation, comme lors de l’installation.',
    },
  },
}
