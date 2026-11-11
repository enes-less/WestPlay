import { describe, expect, it } from 'vitest'
import { addressFor, searchAddress, shownAddress, siteOf } from './address'

describe('the address field', () => {
  it('loads a web address as it is', () => {
    expect(addressFor('https://example.com/a?b=1', 'google')).toBe('https://example.com/a?b=1')
    expect(addressFor('  http://example.com  ', 'google')).toBe('http://example.com/')
  })

  it('makes a site’s name a secure address', () => {
    expect(addressFor('apple.com', 'google')).toBe('https://apple.com/')
    expect(addressFor('www.scssoft.com/news', 'google')).toBe('https://www.scssoft.com/news')
    expect(addressFor('localhost:3000', 'google')).toBe('https://localhost:3000/')
    expect(addressFor('192.168.1.1', 'google')).toBe('https://192.168.1.1/')
    expect(addressFor('münchen.de', 'google')).toBe('https://xn--mnchen-3ya.de/')
  })

  it('searches for words, and for anything that isn’t a web address', () => {
    expect(addressFor('euro truck simulator', 'google')).toBe('https://www.google.com/search?q=euro+truck+simulator')
    expect(addressFor('trucks', 'duckDuckGo')).toBe('https://duckduckgo.com/?q=trucks')
    expect(addressFor('trucks', 'yahoo')).toBe('https://search.yahoo.com/search?p=trucks')
    expect(addressFor('javascript:alert(1)', 'bing')).toBe('https://www.bing.com/search?q=javascript%3Aalert(1)')
    expect(addressFor('file:///C:/Windows', 'ecosia')).toBe(searchAddress('file:///C:/Windows', 'ecosia'))
    expect(addressFor('a.b c', 'google')).toBe('https://www.google.com/search?q=a.b+c')
    expect(addressFor('1.5', 'google')).toBe('https://www.google.com/search?q=1.5')
    expect(addressFor('   ', 'google')).toBeNull()
  })

  it('shows a page by its site, and a search by its words', () => {
    expect(shownAddress('https://www.apple.com/ipad/')).toEqual({ text: 'apple.com', secure: true, search: false })
    expect(shownAddress('http://example.com/')).toEqual({ text: 'example.com', secure: false, search: false })
    expect(shownAddress('https://www.google.com/search?q=euro+truck')).toEqual({
      text: 'euro truck',
      secure: true,
      search: true,
    })
    expect(shownAddress('https://duckduckgo.com/?q=trucks&ia=web').text).toBe('trucks')
    expect(shownAddress('https://www.google.com/maps').text).toBe('google.com')
    expect(siteOf('https://www.wikipedia.org/wiki/Truck')).toBe('wikipedia.org')
  })
})
