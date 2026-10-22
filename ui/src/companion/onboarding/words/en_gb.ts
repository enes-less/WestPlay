import type { Words } from '.'
import { words as american } from './en_us'

/** British English: American English's words, with British punctuation where they differ (single quotes,
 * the full stop outside them). */
export const words: Words = {
  ...american,
  locale: 'en-GB',
  launch: {
    ...american.launch,
    steps: [
      {
        title: 'Start the game',
        text: 'When it says ‘Request to use advanced SDK features detected’, choose OK. That’s WestPlay.',
      },
      american.launch.steps[1],
    ],
  },
}
