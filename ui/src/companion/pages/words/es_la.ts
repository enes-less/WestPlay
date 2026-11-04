import type { CompanionWords } from '.'
import { words as spain } from './es_es'

/** Spanish (Latin America): Spain's Spanish, with Latin American usage where it differs (no "ha" perfect
 * for what just happened, "computadora", "mouse", "presionar", "agregar", "videos", "en vivo", "clima"). */
export const words: CompanionWords = {
  ...spain,
  state: { ...spain.state, stopped: 'WestPlay se detuvo' },
  screen: {
    ...spain.screen,
    titles: { ...spain.screen.titles, stopped: 'Algo falló' },
    subs: {
      ...spain.screen.subs,
      stopped: 'El servicio de WestPlay se detuvo. Tienes los detalles en Información.',
    },
    gpuNote: 'Si la pantalla se traba, elige otra tarjeta: WestPlay se reinicia en ella.',
    elevated: {
      ...spain.screen.elevated,
      text: 'Así WestPlay no puede leer su ruta, sus indicaciones ni el clima. Inicia el juego de forma normal, no como administrador.',
    },
  },
  controls: {
    ...spain.controls,
    sub: 'Mantén presionada una tecla para usar la pantalla del camión con el mouse. Al soltarla, vuelves a mirar por la cabina como siempre.',
    key: 'Mantén presionado para usar la pantalla',
    keyToggle: 'Presiona para usar la pantalla',
    toggle: 'Presionar una vez en lugar de mantener',
    toggleDetail: 'Presiona {key} para usar la pantalla y vuelve a presionarla para mirar por la cabina.',
    pressKey: 'Presiona una tecla…',
    keybindsNote:
      'Haz clic en un atajo y presiona su nueva tecla. Si a la vez mantienes {key}, el atajo solo funciona mientras usas la pantalla. Esc para cancelar.',
  },
  accounts: {
    ...spain.accounts,
    sub: 'Inicia sesión para tener tu música y tus videos en la pantalla del camión. Cada servicio tiene su propia ventana de inicio de sesión, y WestPlay nunca ve los datos de tus cuentas.',
  },
  map: {
    ...spain.map,
    titles: { ...spain.map.titles, failed: 'No se pudo preparar el mapa' },
  },
  setup: {
    ...spain.setup,
    closeGame: { ...spain.setup.closeGame, title: 'Primero cierra el juego.' },
  },
  tutorial: {
    ...spain.tutorial,
    hold: {
      title: 'Mantén presionado {key}',
      sub: 'En la cabina, usas la pantalla mientras mantienes presionada esta tecla. Pruébalo ahora.',
      banner: { title: 'Mantén presionado {key}', text: 'Para usar la pantalla' },
    },
    open: {
      title: 'Haz clic en una app',
      sub: 'No sueltes la tecla: el mouse está ahora en la pantalla.',
      banner: { title: 'El mouse está en la pantalla', text: 'Haz clic en una app' },
    },
    letGo: {
      ...spain.tutorial.letGo,
      sub: 'Al soltarla, vuelves a mirar por la cabina con el mouse. Mientras tanto, sigues manejando con el teclado.',
    },
    holdAgain: { title: 'Mantén presionado {key}', text: 'Para usar la pantalla' },
    tour: {
      ...spain.tutorial.tour,
      controls: 'Cambia la tecla que mantienes presionada y la velocidad del puntero.',
      customize: 'Agrega tus propios fondos de pantalla e iconos de apps, y elige cuáles se muestran.',
    },
  },
  uninstall: {
    ...spain.uninstall,
    text: 'WestPlay se quitará del juego y de tus perfiles, y se eliminará de esta computadora junto con tus inicios de sesión y sus registros.',
    closeGame: { ...spain.uninstall.closeGame, title: 'Primero cierra el juego.' },
    steps: { ...spain.uninstall.steps, data: 'Eliminando WestPlay de esta computadora' },
    profilesFailed:
      'No se pudieron cambiar {n} de tus perfiles, que todavía incluyen WestPlay. Quítalo de ellos en el Gestor de mods.',
  },
  about: { ...spain.about, sub: 'Datos en vivo de WestPlay y del juego, por si algo no va bien.' },
  customize: {
    ...spain.customize,
    add: 'Agregar',
    wallpapersNote:
      'Puedes usar cualquier imagen. Para un fondo de pantalla que cambie entre claro y oscuro, guarda su versión oscura con el mismo nombre y .dark, por ejemplo silk.jpg y silk.dark.jpg. Al agregar uno se agregan los dos.',
    addFolder: 'Agregar desde una carpeta',
    addZip: 'Agregar desde un .zip',
    looksNote:
      'Para un icono que cambie entre claro y oscuro, agrega una versión oscura con .dark en el nombre, por ejemplo maps.dark.png junto a maps.png.',
    added: 'Agregado: {names}',
    nothingAdded: 'No se agregó nada.',
  },
  update: {
    ...spain.update,
    closeGame: { ...spain.update.closeGame, title: 'Primero cierra el juego.' },
    failed: 'No se pudo instalar la actualización.',
  },
}
