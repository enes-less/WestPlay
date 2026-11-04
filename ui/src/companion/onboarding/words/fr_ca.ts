import type { Words } from '.'
import { words as france } from './fr_fr'

/** French (Canada): France's French, with Québec's usage where it differs ("plugiciel", a game that "roule"). */
export const words: Words = {
  ...france,
  locale: 'fr-CA',
  summary: { ...france.summary, chosen: 'Dossier personnalisé' },
  install: {
    ...france.install,
    sub: 'WestPlay ajoute au jeu un plugiciel qui charge ses écrans à chaque partie : rien à activer dans le Gestionnaire de mods. Les fichiers du jeu restent intacts, et la désinstallation de WestPlay retire tout.',
    plugin: 'Plugiciel',
    banners: {
      ...france.install.banners,
      inUse: { title: 'Fermez le jeu', text: 'Il utilise le plugiciel' },
    },
    closeToInstall: {
      title: 'Fermez d’abord le jeu.',
      text: 'WestPlay ne peut pas s’installer pendant que le jeu roule.',
    },
    inUse: {
      title: 'Fermez d’abord le jeu.',
      text: 'Le jeu utilise le fichier du plugiciel, qui ne peut pas être remplacé tant qu’il roule.',
    },
    needsAdministrator: {
      title: 'Autorisation d’administrateur requise.',
      text: 'Le jeu se trouve dans un dossier protégé. Windows demandera l’autorisation une seule fois, uniquement pour copier le plugiciel. Choisissez « Oui ».',
    },
    declined: {
      title: 'L’autorisation a été refusée.',
      text: 'Le plugiciel ne peut pas être installé sans elle. Réessayez et choisissez « Oui ».',
    },
    pluginFailed: 'Le plugiciel n’a pas pu être installé.',
    noPlugin: 'Le plugiciel manque dans cette copie de WestPlay. Réinstallez WestPlay.',
  },
}
