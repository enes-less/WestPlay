import { useEffect, useRef, useState } from 'react'

/**
 * A music app's questions to its web player (Apple Music's, Spotify's, YouTube Music's): each asked of the host
 * with an id and answered with it, kept for the session (going back finds the page as it was), and asked afresh
 * for a page whose answer changes as the service is used.
 */
export interface Asking<Q extends { ask: string }, A extends Record<Q['ask'], unknown>> {
  /** A question's answer: as known this session, or (`fresh`, or not known) asked. */
  ask<T extends Q>(question: T, fresh?: boolean): Promise<A[T['ask']]>
  /** The host's answer to the question asked with `id` (`failed`: not answered). */
  receive(message: { id: number; failed?: boolean } & Partial<A>): void
  /** A question's answer once it comes (`null` until then), whether it failed, and asking again; asked again
   * when the question changes. A page whose answer changes as the service is used gives a `refresh`: it's asked
   * afresh when that changes (the page shown again) and whenever the answer is known already (the page opened
   * anew), the known answer shown meanwhile. */
  useAsked<T extends Q>(
    question: T | null,
    refresh?: number,
  ): { answer: A[T['ask']] | null; failed: boolean; retry(): void }
  /** Forgets every answer (signed in or out: the account's are another's). */
  clear(): void
}

/** How long the service has to answer (it asks its servers in turn). */
const ANSWER_WAIT = 20000

export function createAsking<Q extends { ask: string }, A extends Record<Q['ask'], unknown>>(
  send: (id: number, question: Q) => void,
  preview: (question: Q) => Promise<unknown>,
  inHost: () => boolean,
  /** Each answer as it comes (Spotify keeps some of them for its next run). */
  answered?: (question: Q, answer: unknown) => void,
): Asking<Q, A> {
  const answers = new Map<string, unknown>()
  const waiting = new Map<number, { ask: Q['ask']; resolve(answer: unknown): void; reject(): void }>()
  let nextQuestion = 1

  const askHost = (question: Q) => {
    const id = nextQuestion++
    return new Promise<unknown>((resolve, reject) => {
      waiting.set(id, { ask: question.ask, resolve, reject })
      send(id, question)
      setTimeout(() => {
        if (waiting.delete(id)) reject(new Error('not answered'))
      }, ANSWER_WAIT)
    })
  }

  const ask = <T extends Q>(question: T, fresh = false): Promise<A[T['ask']]> => {
    const key = JSON.stringify(question)
    const known = answers.get(key)
    if (known && !fresh) return Promise.resolve(known as A[T['ask']])
    const asked = inHost() ? askHost(question) : preview(question)
    return asked.then((answer) => {
      answers.set(key, answer)
      answered?.(question, answer)
      return answer as A[T['ask']]
    })
  }

  const receive = (message: { id: number; failed?: boolean } & Partial<A>) => {
    const asked = waiting.get(message.id)
    if (!asked) return
    waiting.delete(message.id)
    const answer = message[asked.ask as keyof A]
    if (message.failed || answer === undefined) asked.reject()
    else asked.resolve(answer)
  }

  function useAsked<T extends Q>(
    question: T | null,
    refresh?: number,
  ): { answer: A[T['ask']] | null; failed: boolean; retry(): void } {
    const key = question ? JSON.stringify(question) : ''
    const [tries, setTries] = useState(0)
    const refreshed = useRef(refresh)
    const [state, setState] = useState<{ key: string; answer: A[T['ask']] | null; failed: boolean }>(() => ({
      key,
      answer: question ? ((answers.get(key) as A[T['ask']] | undefined) ?? null) : null,
      failed: false,
    }))
    useEffect(() => {
      if (!question) return
      let current = true
      const known = answers.get(key) as A[T['ask']] | undefined
      const fresh = refresh !== undefined && (refreshed.current !== refresh || known !== undefined)
      refreshed.current = refresh
      if (fresh && known !== undefined) setState({ key, answer: known, failed: false })
      ask(question, fresh).then(
        (answer) => current && setState({ key, answer, failed: false }),
        // Not answered: what was known stays.
        () => current && known === undefined && setState({ key, answer: null, failed: true }),
      )
      return () => {
        current = false
      }
      // The key stands for the question.
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [key, tries, refresh])
    const mine = state.key === key
    return {
      answer: mine ? state.answer : null,
      failed: mine && state.failed,
      retry: () => {
        setState({ key, answer: null, failed: false })
        setTries((n) => n + 1)
      },
    }
  }

  return { ask, receive, useAsked, clear: () => answers.clear() }
}
