import type { Strings } from '.'
import { strings as spain } from './es_es'

/** Spanish (Latin America): Spain's Spanish, with Latin American usage where it differs (no "ha" perfect
 * for what just happened, "agregar", "computadora", "manejar", "en vivo"; Spotify's "playlist" and "Tus me
 * gusta", iOS's "Clima" and "Configuración"). */
export const strings: Strings = {
  ...spain,
  locale: 'es-419',
  turns: { ...spain.turns, 'u-turn': 'Da vuelta en U' },
  destinations: {
    ...spain.destinations,
    kinds: { ...spain.destinations.kinds, parking: 'Lugar de descanso' },
    nearby: { ...spain.destinations.nearby, parking: 'Lugares de descanso' },
    stopAdds: (distance, time) => `Parada agregada · ${distance} y ${time} más`,
    notOnRoute: 'Parada agregada, pero la ruta del juego no pasa por ella',
    addStop: 'Agregar parada',
    done: { ...spain.destinations.done, addStop: 'Parada agregada' },
    noAnswer: 'El juego no respondió',
    failed: 'El juego no encontró una ruta hasta ahí',
  },
  keyboard: { ...spain.keyboard, pausedWhileDriving: 'Teclado en pausa mientras manejas' },
  openMusicApp: 'Abre Apple Music, Spotify o YouTube Music en la computadora.',
  youtube: {
    ...spain.youtube,
    chips: [
      { label: 'Camiones', query: 'camiones' },
      { label: 'Música', query: 'música' },
      { label: 'Podcasts', query: 'podcast' },
      { label: 'Videojuegos', query: 'videojuegos' },
      { label: 'En vivo', query: 'en vivo' },
    ],
    failed: 'YouTube no respondió.',
    tryAgain: 'Volver a intentar',
    live: 'EN VIVO',
    finishOnPc:
      'Termina de iniciar sesión en la computadora: el inicio de sesión de Google está abierto en una ventana del escritorio.',
  },
  spotify: {
    ...spain.spotify,
    finishOnPc:
      'Termina de iniciar sesión en la computadora: el inicio de sesión de Spotify está abierto en una ventana del escritorio.',
    filters: { ...spain.spotify.filters, podcasts: 'Podcasts' },
    noResults: (query) => `No se encontraron resultados para “${query}”`,
    noResultsHint: 'Verifica la ortografía o intenta con menos palabras o con otras diferentes.',
    playlists: 'Playlists',
    podcasts: 'Podcasts y programas',
    kinds: { ...spain.spotify.kinds, playlist: 'Playlist', liked: 'Playlist', show: 'Podcast' },
    playingFrom: {
      ...spain.spotify.playingFrom,
      playlist: 'Reproduciendo desde la playlist',
      show: 'Reproduciendo desde el podcast',
    },
    cantPlay: 'Spotify no puede reproducir este contenido en este momento.',
    failed: 'Algo salió mal',
    tryAgain: 'Intentar de nuevo',
    addToLiked: 'Agregar a Tus me gusta',
    addToLibrary: 'Agregar a Tu biblioteca',
    added: 'Se agregó a Tus me gusta.',
    removed: 'Se eliminó de Tus me gusta.',
    addedToLibrary: 'Se agregó a Tu biblioteca.',
    removedFromLibrary: 'Se eliminó de Tu biblioteca.',
    previewText:
      'Regístrate para disfrutar canciones y podcasts ilimitados con anuncios ocasionales. No necesitas tarjeta de crédito.',
    prompts: {
      play: { title: 'Comienza a escuchar con una cuenta gratuita de Spotify', text: '' },
      save: {
        title: 'Guardar en Tu biblioteca',
        text: 'Inicia sesión para guardar canciones, álbumes, playlists y artistas.',
      },
      playlist: { title: 'Crear una playlist', text: 'Inicia sesión para crear y compartir playlists.' },
    },
    firstPlaylist: {
      title: 'Crea tu primera playlist',
      text: 'Es muy fácil, te vamos a ayudar',
      button: 'Crear playlist',
    },
    firstPodcasts: {
      title: 'Busquemos podcasts para seguir',
      text: 'Te avisaremos cuando haya episodios nuevos',
      button: 'Explorar podcasts',
    },
  },
  appleMusic: {
    ...spain.appleMusic,
    stations: 'Estaciones',
    noResultsHint: 'Intenta con otra búsqueda.',
    recentlyAdded: 'Agregado recientemente',
    kinds: { ...spain.appleMusic.kinds, station: 'Estación' },
    live: 'EN VIVO',
    favorited: 'Agregado a Favoritos',
    addToLibrary: 'Agregar a la biblioteca',
    added: 'Agregado a la biblioteca',
    finishOnPc:
      'Termina de iniciar sesión en la computadora: el inicio de sesión de Apple Music está abierto en una ventana del escritorio.',
    homePitch: {
      title: 'Aprovecha Apple Music al máximo',
      text: 'Inicia sesión para ver recomendaciones para ti, lo último que escuchaste y tu biblioteca.',
    },
    libraryPitch: { title: 'Tu biblioteca', text: 'Inicia sesión para ver la música que agregaste.' },
    promptFor: { ...spain.appleMusic.promptFor, library: 'Inicia sesión para agregar música a tu biblioteca.' },
    cantPlay: 'Apple Music no puede reproducir esto en este momento.',
    failed: 'No se pudo cargar.',
    tryAgain: 'Volver a intentar',
  },
  youtubeMusic: {
    ...spain.youtubeMusic,
    searchHint: 'Busca canciones, artistas, podcasts',
    video: 'Video',
    finishOnPc: 'Termina de iniciar sesión en la computadora.',
    cantPlay: 'YouTube Music no puede reproducir esto en este momento.',
    failed: 'No se pudo cargar.',
    tryAgain: 'Volver a intentar',
    addToQueue: 'Agregar a la fila',
    addedToQueue: 'Se agregó la canción a la fila',
  },
  weatherApp: { ...spain.weatherApp, unavailable: 'El clima aparece mientras el juego está abierto.' },
  settings: { ...spain.settings, showAlbumArt: 'Mostrar portadas' },
  apps: { ...spain.apps, weather: 'Clima', settings: 'Configuración' },
  safari: {
    ...spain.safari,
    searchOrEnter: 'Buscar o ingresar nombre del sitio web',
    notConnected: 'Safari no puede abrir la página porque la computadora no está conectada a Internet.',
    timedOut: 'Safari no puede abrir la página porque el servidor dejó de responder.',
    tryAgain: 'Volver a intentar',
    searchEngine: 'Motor de búsqueda',
  },
}
