/** What turns the field's text `from` into `to`, as keys: Backspace for each character after what they share,
 * then the rest typed. */
export function typedSteps(
  from: string,
  to: string,
): ({ action: 'key'; key: 'backspace' } | { action: 'type'; text: string })[] {
  const a = Array.from(from)
  const b = Array.from(to)
  let same = 0
  while (same < a.length && same < b.length && a[same] === b[same]) same++
  const steps: ({ action: 'key'; key: 'backspace' } | { action: 'type'; text: string })[] = []
  for (let i = same; i < a.length; i++) steps.push({ action: 'key', key: 'backspace' })
  if (same < b.length) steps.push({ action: 'type', text: b.slice(same).join('') })
  return steps
}
