import type { Words } from '.'
import { words as polish } from './pl_pl'

/**
 * Silesian: Polish's words for now (Silesian's speakers read Polish, and a poorly written Silesian would be
 * worse than none: it waits for a native speaker), with the game's own Silesian wherever the game's text is
 * quoted, so it matches what the player sees there. Polish's plural rules (Intl has none for Silesian).
 */
export const words: Words = {
  ...polish,
  install: {
    ...polish.install,
    sub: 'WestPlay dodaje do gry wtyczkę, która przy każdej grze sama wczytuje ekrany WestPlay dla ciężarówek: w menu „Mynedżer modōw” nie trzeba niczego włączać. Pliki samej gry pozostają nietknięte, a odinstalowanie WestPlay usuwa wszystko.',
  },
  launch: {
    ...polish.launch,
    steps: [
      {
        title: polish.launch.steps[0].title,
        text: 'Gdy pojawi się komunikat „Wykrytŏ prōbã skorzistaniŏ ze zaawansowanych nŏczyniōw deweloperskich.”, wybierz „Ok”. To WestPlay.',
      },
      polish.launch.steps[1],
    ],
  },
}
