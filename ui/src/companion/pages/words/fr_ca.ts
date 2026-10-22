import type { CompanionWords } from '.'
import { words as france } from './fr_fr'

/** French (Canada): France's French, with Québec's usage where it differs ("plugiciel"). */
export const words: CompanionWords = {
  ...france,
  setup: {
    ...france.setup,
    plugin: 'Plugiciel',
    reinstalled: 'Réinstallé',
    checking: { title: 'Vérification de votre jeu', text: 'Version, plugiciel et mods' },
  },
  uninstall: {
    ...france.uninstall,
    steps: { ...france.uninstall.steps, plugin: 'Suppression du plugiciel' },
  },
  about: { ...france.about, log: 'Journal du plugiciel' },
}
