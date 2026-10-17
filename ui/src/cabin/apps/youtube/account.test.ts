import { describe, expect, it } from 'vitest'
import { keptAccount } from './account'

describe('keptAccount', () => {
  it('keeps signed in and a picture on Google image hosts', () => {
    expect(keptAccount({ signedIn: true, avatar: 'https://yt3.ggpht.com/me=s88' })).toEqual({
      signedIn: true,
      avatar: 'https://yt3.ggpht.com/me=s88',
    })
  })

  it('drops a picture from anywhere else, or when signed out', () => {
    expect(keptAccount({ signedIn: true, avatar: 'https://evil.example/me.png' })).toEqual({
      signedIn: true,
      avatar: null,
    })
    expect(keptAccount({ signedIn: false, avatar: 'https://yt3.ggpht.com/me' })).toEqual({
      signedIn: false,
      avatar: null,
    })
  })

  it('keeps nothing of anything else', () => {
    expect(keptAccount(null)).toBeNull()
    expect(keptAccount('signed in')).toBeNull()
    expect(keptAccount({ signedIn: 'yes', avatar: null })).toBeNull()
  })
})
