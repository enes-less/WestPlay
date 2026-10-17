import type { Words } from '.'
import { words as france } from './fr_fr'

/** French (Canada): France's French, with Québec's usage where it differs. */
export const words: Words = {
  ...france,
  locale: 'fr-CA',
  summary: { ...france.summary, chosen: 'Dossier personnalisé' },
  install: {
    ...france.install,
    sub: 'WestPlay ajoute deux fichiers à votre jeu : un plugiciel et un mod pour les écrans des camions. Les fichiers du jeu lui-même restent intacts.',
    plugin: 'Plugiciel',
    banners: {
      ...france.install.banners,
      installing: { title: 'Installation…', text: 'Plugiciel et mod des écrans' },
      inUse: { title: 'Fermez le jeu', text: 'Il utilise le plugiciel' },
    },
    inUse: {
      title: 'Veuillez fermer le jeu.',
      text: 'Il utilise le fichier du plugiciel, qui ne peut donc pas être remplacé tant que le jeu roule.',
    },
    needsAdministrator: {
      title: 'Autorisation d’administrateur requise.',
      text: 'Votre jeu se trouve dans un dossier protégé. Windows demandera l’autorisation une seule fois, uniquement pour copier le plugiciel. Choisissez « Oui ».',
    },
    declined: {
      title: 'L’autorisation n’a pas été accordée.',
      text: 'Le plugiciel ne peut pas être installé sans elle. Réessayez et choisissez « Oui ».',
    },
    pluginFailed: 'Le plugiciel n’a pas pu être installé.',
    noPlugin: 'Il manque le plugiciel dans cette copie de WestPlay. Veuillez réinstaller WestPlay.',
  },
}
