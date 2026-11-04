import type { Strings } from '.'
import { strings as france } from './fr_fr'

/** French (Canada): France's French, with Québec's usage where it differs (the game's "traversier" and
 * "halte routière", Apple's "Balados"). */
export const strings: Strings = {
  ...france,
  locale: 'fr-CA',
  destinations: {
    ...france.destinations,
    kinds: { ...france.destinations.kinds, parking: 'Halte routière', ferry: 'Traversier' },
    nearby: { ...france.destinations.nearby, parking: 'Haltes routières', ferry: 'Traversiers' },
  },
  youtube: {
    ...france.youtube,
    chips: [
      { label: 'Camions', query: 'camionnage' },
      { label: 'Musique', query: 'musique' },
      { label: 'Balados', query: 'balado' },
      { label: 'Jeux vidéo', query: 'jeux vidéo' },
      { label: 'En direct', query: 'en direct' },
    ],
  },
  apps: { ...france.apps, podcasts: 'Balados' },
}
