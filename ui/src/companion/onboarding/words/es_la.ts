import type { Words } from '.'
import { words as spain } from './es_es'

/** Spanish (Latin America): Spain's Spanish, with Latin American usage where it differs (no "ha" perfect
 * for what just happened, "computadora", "mouse", "presionar"). */
export const words: Words = {
  ...spain,
  locale: 'es-419',
  game: {
    ...spain.game,
    found: 'WestPlay encontró Euro Truck Simulator 2 en esta computadora.',
    foundMany: 'Euro Truck Simulator 2 está instalado en más de un lugar. Elige el que usas para jugar.',
    missing:
      'WestPlay no encontró Euro Truck Simulator 2. Elige la carpeta donde lo instalaste, la que contiene base.scs.',
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
      failed: { title: 'La instalación no terminó', text: 'Mira los detalles a la izquierda' },
    },
    declined: {
      title: 'No se concedió el permiso.',
      text: 'Sin él no se puede instalar el plugin. Vuelve a intentarlo y elige «Sí».',
    },
    pluginFailed: 'No se pudo instalar el plugin.',
    modFailed: 'No se pudieron preparar las pantallas de WestPlay para el camión a partir de tu juego.',
    noDocuments: 'No se encontró tu carpeta Documentos.',
  },
  map: {
    ...spain.map,
    banners: {
      ...spain.map.banners,
      failed: { title: 'No se pudo preparar el mapa', text: 'WestPlay lo volverá a intentar más tarde' },
    },
  },
  controls: {
    ...spain.controls,
    title: 'Mantén presionado para usar la pantalla',
    sub: 'Mientras mantengas presionada esta tecla, el mouse controla la pantalla del camión. Al soltarla, vuelve a mirar por la cabina como siempre.',
    keys: {
      leftAlt: 'Alt izquierda',
      rightCtrl: 'Ctrl derecha',
      mouseBack: 'Botón Atrás del mouse',
      mouseForward: 'Botón Adelante del mouse',
    },
  },
}
