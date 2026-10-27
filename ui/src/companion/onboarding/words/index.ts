import type { MapStage } from '../../useHostStatus'
import type { KeyPreset } from '../../useSettings'

/**
 * The onboarding's words, in each of the game's 43 languages (by the game's own codes: `locale/<code>` in
 * its archives). It speaks the game's language, as WestPlay does in the cab; before the game was ever
 * started, Windows' ([`languageOf`]); English for a language the game doesn't have. Where the onboarding
 * names something in the game (its Mod Manager, its question about advanced SDK features, its OK button),
 * it uses the game's own words for it in that language, from the game's `localization.sui`
 * (`mod_browser_t`, `sdk_warning`, `ok`).
 *
 * The translations other than English and Turkish were written without a native speaker's review yet.
 */

/** A note: its first words in bold, then the rest. Also a banner on the screen: its title, then a line. */
export interface Note {
  title: string
  text: string
}

/** A count's words by the language's plural category (`{n}`: the count); `other` is always there. */
export type Plural = Partial<Record<Intl.LDMLPluralRule, string>> & { other: string }

export interface Words {
  /** BCP 47: plural rules, numbers, lists, and `lang` (text set in capitals follows the language's rules). */
  locale: string
  back: string
  continue: string
  welcome: {
    title: string
    sub: string
    start: string
  }
  /** A game install, in a line: "Version 1.61.1.9 · Steam · 34 DLCs". */
  summary: {
    /** `{version}` */
    version: string
    versionUnknown: string
    steam: string
    installed: string
    chosen: string
    dlcs: Plural
  }
  game: {
    title: string
    titleMissing: string
    found: string
    foundMany: string
    missing: string
    chooseAnother: string
    chooseFolder: string
    /** Windows' folder picker's title. */
    picker: string
    /** The banner when there's no game yet (its title is `titleMissing`). */
    bannerMissing: string
    notTheGame: string
    gameGone: string
    closeToGoOn: Note
  }
  check: {
    looking: string
    lookingSub: string
    /** Banners. */
    checking: string
    checked: string
    notYet: string
    title: string
    titleNotYet: string
    cantInstall: string
    version: string
    unknown: string
    dlcs: string
    mapDlcs: string
    noMapDlcs: string
    untested: string
    /** Another CarPlay app in the game (YMN One), which takes the trucks' GPS screens: to be removed. */
    otherCarPlay: { title: string; text: string }
    trucks: string
    truckNotYet: string
    /** Under the trucks: why only these (WestPlay is on the trucks that have CarPlay in real life). */
    otherTrucks: string
    notInMods: string
    notInGame: string
    /** The trucks' names that have words in them; the others are names alone. */
    scania7: string
    scaniaSmartDash: string
    /** The tablet's row: any truck, with WestPlay's tablet (a dashboard toy, its name in the game's shop). */
    anyTruckTablet: string
    profiles: string
    profileCount: Plural
    noProfiles: string
    noProfilesDetail: string
    baseMap: string
    /** `{map}` with `{mods}` */
    withMods: string
    unreadable: Plural
    mapNote: string
  }
  install: {
    title: string
    titleDone: string
    sub: string
    install: string
    tryAgain: string
    asAdministrator: string
    plugin: string
    pluginDetail: string
    banners: {
      /** Its line is the plugin's name (`plugin`). */
      installing: { title: string }
      done: Note
      permission: Note
      inUse: Note
      failed: Note
      /** Its line is what the plugin does (`pluginDetail`). */
      ready: { title: string }
    }
    closeToInstall: Note
    inUse: Note
    needsAdministrator: Note
    declined: Note
    pluginFailed: string
    modFailed: string
    noPlugin: string
    noDocuments: string
  }
  map: {
    title: string
    titleReady: string
    sub: string
    banners: {
      /** Its line is the percentage. */
      building: string
      ready: Note
      waiting: Note
      failed: Note
      getting: string
    }
    waiting: Note
    /** What the map's build is doing, as it does it (`now`) and once it's done (`done`). */
    stages: Record<MapStage, { now: string; done: string }>
  }
  controls: {
    title: string
    sub: string
    keys: Record<KeyPreset, string>
    /** The keys' own labels, as on the language's keyboards. */
    alt: string
    ctrl: string
  }
  accounts: {
    title: string
    sub: string
    signIn: string
    signOut: string
    signedIn: string
    notSignedIn: string
    finishInWindow: string
    notNow: string
    signingIn: Note
  }
  /** Spotify's sign-in step (its buttons and states are `accounts`'). */
  spotify: {
    title: string
    sub: string
    finishInWindow: string
    signingIn: Note
  }
  /** Apple Music's sign-in step, as Spotify's. */
  appleMusic: {
    title: string
    sub: string
    finishInWindow: string
    signingIn: Note
  }
  launch: {
    title: string
    done: string
    steps: [Note, Note]
  }
}

const WORDS: Record<string, Words> = Object.fromEntries(
  Object.entries(import.meta.glob<Words>('./[a-z][a-z]_[a-z][a-z].ts', { eager: true, import: 'words' })).map(
    ([path, words]) => [path.slice(2, -3), words],
  ),
)

/** The game's language codes, for a language Windows shows (BCP 47): its region or script decides between a
 * language's variants, as the game has them. */
const BY_LANGUAGE: Record<string, string | ((tag: string) => string)> = {
  bg: 'bg_bg',
  ca: 'ca_es',
  cs: 'cs_cz',
  da: 'da_dk',
  de: 'de_de',
  el: 'el_gr',
  en: (tag) => (/-(us|ph|um|as|gu|pr|vi)\b/i.test(tag) || tag.toLowerCase() === 'en' ? 'en_us' : 'en_gb'),
  es: (tag) => (/-(es|ad)\b/i.test(tag) || tag.toLowerCase() === 'es' ? 'es_es' : 'es_la'),
  et: 'et_ee',
  eu: 'eu_es',
  fi: 'fi_fi',
  fr: (tag) => (/-ca\b/i.test(tag) ? 'fr_ca' : 'fr_fr'),
  gl: 'gl_es',
  hr: 'hr_hr',
  hu: 'hu_hu',
  it: 'it_it',
  ja: 'ja_jp',
  ka: 'ka_ge',
  ko: 'ko_kr',
  lt: 'lt_lt',
  lv: 'lv_lv',
  mk: 'mk_mk',
  nl: 'nl_nl',
  nb: 'no_no',
  nn: 'no_no',
  no: 'no_no',
  pl: 'pl_pl',
  szl: 'pl_si',
  pt: (tag) => (/-br\b/i.test(tag) ? 'pt_br' : 'pt_pt'),
  ro: 'ro_ro',
  ru: 'ru_ru',
  sk: 'sk_sk',
  sl: 'sl_sl',
  // The game's `sr_sp` is Serbian in Latin letters, `sr_sr` in Cyrillic (Windows' default for Serbian).
  sr: (tag) => (/-latn\b/i.test(tag) ? 'sr_sp' : 'sr_sr'),
  sv: 'sv_se',
  tr: 'tr_tr',
  uk: 'uk_uk',
  vi: 'vi_vn',
  zh: (tag) => (/-(hant|tw|hk|mo)\b/i.test(tag) ? 'zh_tw' : 'zh_cn'),
}

/** The game's code for the first of `tags` (Windows' languages, most preferred first) it has; `en_us`
 * otherwise. */
export function languageOf(tags: readonly string[]): string {
  for (const tag of tags) {
    const known = BY_LANGUAGE[tag.split('-')[0].toLowerCase()]
    const code = typeof known === 'function' ? known(tag) : known
    if (code && WORDS[code]) return code
  }
  return 'en_us'
}

/** The words in the game's language `code` (English for one without). */
export function wordsFor(code: string): Words {
  return WORDS[code] ?? WORDS.en_us
}

/** The languages there are words for. */
export function languages(): string[] {
  return Object.keys(WORDS).sort()
}

/** `template` with each `{name}` filled in. */
export function fill(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (whole, name: string) => (name in values ? String(values[name]) : whole))
}

/** `count` in words, by the language's plural rules. */
export function count(words: Words, plural: Plural, n: number): string {
  const form = plural[new Intl.PluralRules(words.locale).select(n)] ?? plural.other
  return fill(form, { n: new Intl.NumberFormat(words.locale).format(n) })
}

/** Names in a list, as the language writes one ("A, B and C"). */
export function list(words: Words, names: string[]): string {
  return new Intl.ListFormat(words.locale, { style: 'long', type: 'conjunction' }).format(names)
}

export function percent(words: Words, done: number): string {
  return new Intl.NumberFormat(words.locale, { style: 'percent' }).format(done)
}
