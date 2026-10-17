import type { CompanionWords } from '.'
import { words as spain } from './es_es'

/** Spanish (Latin America): Spain's Spanish, with Latin American usage where it differs (no "ha" perfect
 * for what just happened, "computadora", "mouse", "presionar", "videos"). */
export const words: CompanionWords = {
  ...spain,
  screen: {
    ...spain.screen,
    titles: { ...spain.screen.titles, stopped: 'Algo falló' },
    subs: {
      ...spain.screen.subs,
      stopped: 'El motor de WestPlay se detuvo. Tienes los detalles en Información.',
    },
  },
  controls: {
    ...spain.controls,
    sub: 'Mantén presionada una tecla para usar la pantalla del camión con el mouse. Al soltarla, vuelves a mirar por la cabina como siempre.',
    key: 'Mantén presionado para usar la pantalla',
  },
  accounts: {
    ...spain.accounts,
    sub: 'Inicia sesión para ver tu música y tus videos en la pantalla del camión. Inicias sesión en cada servicio en su propia ventana, y WestPlay nunca ve tu cuenta.',
  },
  map: {
    ...spain.map,
    titles: { ...spain.map.titles, failed: 'No se pudo preparar el mapa' },
  },
  tutorial: {
    ...spain.tutorial,
    hold: {
      title: 'Mantén presionado {key}',
      sub: 'En tu cabina, usas la pantalla mientras mantienes presionada esta tecla. Pruébalo ahora.',
      banner: { title: 'Mantén presionado {key}', text: 'Para usar la pantalla' },
    },
    open: {
      title: 'Haz clic en una app',
      sub: 'Sigue presionando: el mouse está ahora en la pantalla.',
      banner: { title: 'El mouse está en la pantalla', text: 'Haz clic en una app' },
    },
    letGo: {
      ...spain.tutorial.letGo,
      sub: 'Suelta y el mouse vuelve a mirar por la cabina. Tus teclas conducen el camión todo el tiempo.',
    },
    holdAgain: { title: 'Mantén presionado {key}', text: 'Para usar la pantalla' },
    tour: { ...spain.tutorial.tour, controls: 'Cambia la tecla que mantienes presionada y la velocidad del puntero.' },
  },
  uninstall: {
    ...spain.uninstall,
    text: 'WestPlay se quita de tu juego y de tus perfiles, y se elimina de esta computadora junto con tus inicios de sesión y sus registros.',
    steps: { ...spain.uninstall.steps, data: 'Eliminando WestPlay de esta computadora' },
    profilesFailed:
      'No se pudieron cambiar {n} de tus perfiles y todavía incluyen WestPlay. Quítalo allí en el Gestor de mods.',
  },
}
