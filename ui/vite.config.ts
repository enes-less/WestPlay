/// <reference types="vitest/config" />
import { createReadStream, existsSync, readFileSync, readdirSync } from 'node:fs'
import { extname, resolve, sep } from 'node:path'
import react from '@vitejs/plugin-react'
import { type Plugin, defineConfig } from 'vite'

/**
 * Development-only folders (never shipped): Apple's/YouTube's own icons and wallpapers, and map data
 * exported from the developer's own game (players' maps come from their own games).
 */
const PLACEHOLDER_FOLDERS = [
  'public/placeholder-icons',
  'public/placeholder-icons-dark',
  'public/placeholder-wallpapers',
  'public/dev-map',
]

/**
 * Release builds (WESTPLAY_RELEASE=1, used by `tauri build`) must not contain the development placeholders
 * (see src/shared/icons.ts and src/shared/wallpapers.ts). Vite copies public/ into the build, so those
 * folders must be gone or empty before a release can be built.
 */
function noPlaceholders(): Plugin {
  return {
    name: 'westplay-no-placeholders',
    apply: 'build',
    buildStart() {
      if (process.env.WESTPLAY_RELEASE !== '1') return
      for (const relative of PLACEHOLDER_FOLDERS) {
        const folder = resolve(import.meta.dirname, relative)
        if (existsSync(folder) && readdirSync(folder).length > 0) {
          this.error(
            `Release build blocked: ${relative} still contains placeholders. Replace them with WestPlay's own assets first.`,
          )
        }
      }
    },
  }
}

/**
 * The development map (public/dev-map, see src/cabin/maps/mapData.ts), served straight from disk: Vite
 * doesn't watch it (see `server.watch`), so it wouldn't know of an export made after it started. A tile
 * the export left out holds nothing (open sea, drawn as the map's ground), so it's answered empty rather
 * than with the page Vite gives unknown addresses.
 */
function devMap(): Plugin {
  const ROOT = resolve(import.meta.dirname, 'public/dev-map')
  const TILE = /\/tiles(?:-\d+)?\/\d+\/\d+\/\d+\.pbf$/
  const TYPES: Record<string, string> = {
    '.pbf': 'application/x-protobuf',
    '.json': 'application/json',
    '.png': 'image/png',
  }
  return {
    name: 'westplay-dev-map',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const path = decodeURIComponent(request.url?.split('?')[0] ?? '')
        if (!path.startsWith('/dev-map/')) return next()
        const file = resolve(ROOT, `.${path.slice('/dev-map'.length)}`)
        const type = TYPES[extname(file)]
        if (!file.startsWith(ROOT + sep) || !type) return next()
        if (!existsSync(file)) {
          response.statusCode = TILE.test(path) ? 204 : 404
          response.end()
          return
        }
        response.setHeader('Content-Type', type)
        // A new export is a new folder, but the manifest naming it is rewritten in place.
        response.setHeader('Cache-Control', 'no-cache')
        createReadStream(file).pipe(response)
      })
    },
  }
}

// Two pages: the companion window (index.html, shown by Tauri) and the in-cabin screen
// (cabin.html, rendered offscreen by the WestPlay host and streamed onto the truck's GPS screen).
/** WestPlay's version: the workspace's (Cargo.toml), for the page outside the app (the preview). */
const VERSION = /^version = "([^"]+)"/m.exec(readFileSync(resolve(import.meta.dirname, '../Cargo.toml'), 'utf8'))?.[1]

export default defineConfig({
  define: { __WESTPLAY_VERSION__: JSON.stringify(VERSION ?? '') },
  plugins: [react(), noPlaceholders(), devMap()],
  clearScreen: false,
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
  server: {
    port: 5173,
    strictPort: true,
    // The development map's tiles are tens of thousands of files (and more with each export kept): watching
    // them only slows the server's start. They're served from disk all the same.
    watch: { ignored: ['**/public/dev-map/**'] },
  },
  build: {
    target: 'es2023',
    rollupOptions: {
      input: {
        companion: resolve(import.meta.dirname, 'index.html'),
        cabin: resolve(import.meta.dirname, 'cabin.html'),
      },
    },
  },
})
