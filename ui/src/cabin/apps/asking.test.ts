import { describe, expect, it } from 'vitest'
import { createAsking } from './asking'

type Question = { ask: 'home'; page: number } | { ask: 'library' }
type Answers = { home: string; library: string[] }

/** An asking whose host is a list of sent questions, answered by hand. */
function host(answered?: (question: Question, answer: unknown) => void) {
  const sent: { id: number; question: Question }[] = []
  const asking = createAsking<Question, Answers>(
    (id, question) => sent.push({ id, question }),
    () => Promise.reject(new Error('no preview here')),
    () => true,
    answered,
  )
  return { asking, sent }
}

describe('asking', () => {
  it('answers each question with the answer sent back under its id', async () => {
    const { asking, sent } = host()
    const home = asking.ask({ ask: 'home', page: 1 })
    const library = asking.ask({ ask: 'library' })
    asking.receive({ id: sent[1].id, library: ['a playlist'] })
    asking.receive({ id: sent[0].id, home: 'Home' })
    await expect(home).resolves.toBe('Home')
    await expect(library).resolves.toEqual(['a playlist'])
  })

  it('keeps answers for the session, and asks again only when asked for fresh', async () => {
    const { asking, sent } = host()
    const first = asking.ask({ ask: 'home', page: 1 })
    asking.receive({ id: sent[0].id, home: 'Home' })
    await first
    await expect(asking.ask({ ask: 'home', page: 1 })).resolves.toBe('Home')
    expect(sent).toHaveLength(1)
    void asking.ask({ ask: 'home', page: 1 }, true)
    expect(sent).toHaveLength(2)
  })

  it('hands each answer as it comes to `answered` (Spotify keeps some), but not those known already', async () => {
    const seen: [Question, unknown][] = []
    const { asking, sent } = host((question, answer) => seen.push([question, answer]))
    const library = asking.ask({ ask: 'library' })
    asking.receive({ id: sent[0].id, library: ['a playlist'] })
    await library
    await asking.ask({ ask: 'library' })
    expect(seen).toEqual([[{ ask: 'library' }, ['a playlist']]])
  })

  it('fails a question its answer says failed, and forgets everything when cleared', async () => {
    const { asking, sent } = host()
    const failed = asking.ask({ ask: 'library' })
    asking.receive({ id: sent[0].id, failed: true })
    await expect(failed).rejects.toBeUndefined()
    const home = asking.ask({ ask: 'home', page: 1 })
    asking.receive({ id: sent[1].id, home: 'Home' })
    await home
    asking.clear()
    void asking.ask({ ask: 'home', page: 1 })
    expect(sent).toHaveLength(3)
  })
})
