import type { Words } from '.'
import { words as spain } from './es_es'

/** Spanish (Latin America): Spain's Spanish, with Latin American usage where it differs (no "ha" perfect
 * for what just happened, "computadora", "mouse", "presionar", Spotify's "playlists"). */
export const words: Words = {
  ...spain,
  locale: 'es-419',
  game: {
    ...spain.game,
    found: 'WestPlay encontró el juego en esta computadora.',
    foundMany: 'El juego está instalado en más de un lugar. Elige el que usas para jugar.',
    foundBoth: 'WestPlay encontró los dos juegos en esta computadora y se instalará en ambos.',
    missing: 'WestPlay no encontró el juego. Elige la carpeta donde está instalado, la que contiene base.scs.',
    closeToGoOn: { ...spain.game.closeToGoOn, title: 'Primero cierra el juego.' },
  },
  check: {
    ...spain.check,
    unreadable: {
      one: 'No se pudo leer {n} mod de mapa, así que sus cambios no aparecerán',
      other: 'No se pudieron leer {n} mods de mapa, así que sus cambios no aparecerán',
    },
  },
  install: {
    ...spain.install,
    banners: {
      ...spain.install.banners,
      failed: { title: 'La instalación falló', text: 'Tienes los detalles a la izquierda' },
    },
    closeToInstall: { ...spain.install.closeToInstall, title: 'Primero cierra el juego.' },
    inUse: { ...spain.install.inUse, title: 'Primero cierra el juego.' },
    declined: {
      title: 'No se concedió el permiso.',
      text: 'Sin él no se puede instalar el plugin. Vuelve a intentarlo y elige «Sí».',
    },
    pluginFailed: 'No se pudo instalar el plugin.',
    modFailed: 'No se pudieron preparar las pantallas de WestPlay a partir de los archivos del juego.',
    noDocuments: 'No se encontró la carpeta Documentos.',
  },
  map: {
    ...spain.map,
    banners: {
      ...spain.map.banners,
      failed: { title: 'No se pudo preparar el mapa', text: 'WestPlay lo volverá a intentar más tarde' },
    },
    waiting: { ...spain.map.waiting, title: 'Primero cierra el juego.' },
  },
  controls: {
    ...spain.controls,
    title: 'Mantén presionado para usar la pantalla',
    sub: 'Mientras mantengas presionada esta tecla, el mouse maneja la pantalla del camión. Al soltarla, vuelves a mirar por la cabina como siempre.',
    keys: {
      leftAlt: 'Alt izquierda',
      rightCtrl: 'Ctrl derecha',
      mouseBack: 'Botón Atrás del mouse',
      mouseForward: 'Botón Adelante del mouse',
    },
  },
  spotify: {
    ...spain.spotify,
    sub: 'Opcional. Inicia sesión para escuchar Spotify en la pantalla del camión, con tus playlists y tu biblioteca. El inicio de sesión se hace en otra ventana de Spotify, y WestPlay nunca ve los datos de tu cuenta.',
  },
  launch: {
    ...spain.launch,
    steps: [
      spain.launch.steps[0],
      spain.launch.steps[1],
      {
        title: 'A la carretera',
        text: 'Mantén presionada tu tecla para usar la pantalla y suéltala para mirar por la cabina. ¡Buen viaje!',
      },
    ],
  },
}
