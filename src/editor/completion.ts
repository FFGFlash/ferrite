/**
 * Completion filtering. The backend ranks and trims the server's items for
 * what was typed when they were requested; as typing continues, the popup
 * narrows them here with the same rules (`match_tier` in
 * `src-tauri/src/lsp/commands.rs`) instead of asking again.
 */
import type { Position } from '../ipc/editor'
import type { CompletionItem } from '../ipc/lsp'

/**
 * How well `candidate` matches `typed`: 0 for a prefix, 1 for a prefix
 * ignoring case, 2 for its letters in order; `null` for no match.
 */
export function matchTier(candidate: string, typed: string): number | null {
  if (candidate.startsWith(typed)) {
    return 0
  }
  const lower = candidate.toLowerCase()
  const wanted = typed.toLowerCase()
  if (lower.startsWith(wanted)) {
    return 1
  }
  let at = 0
  for (const ch of wanted) {
    at = lower.indexOf(ch, at)
    if (at < 0) {
      return null
    }
    at += ch.length
  }
  return 2
}

/** Whether typing `ch` continues a word (and so keeps completion open). */
export function isWordChar(ch: string): boolean {
  return /^[\p{L}\p{N}_$]$/u.test(ch)
}

/** The word containing the character at `index` of `text`, as `[from, to)`; `null` off a word. */
export function wordAround(text: string, index: number): [number, number] | null {
  if (!isWordChar(text.charAt(index))) {
    return null
  }
  let from = index
  let to = index + 1
  while (from > 0 && isWordChar(text.charAt(from - 1))) {
    from--
  }
  while (to < text.length && isWordChar(text.charAt(to))) {
    to++
  }
  return [from, to]
}

/**
 * The items matching what was typed between each item's start and the
 * cursor on `lineText`, best first; `null` if the cursor left the range the
 * items apply to.
 */
export function visibleCompletions(
  items: readonly CompletionItem[],
  cursor: Position,
  lineText: string,
): CompletionItem[] | null {
  const ranked: { item: CompletionItem; tier: number; index: number }[] = []
  let applies = false
  items.forEach((item, index) => {
    if (item.from.line !== cursor.line || item.from.column > cursor.column) {
      return
    }
    applies = true
    const typed = lineText.slice(item.from.column, cursor.column)
    const tier = matchTier(item.filter, typed)
    if (tier !== null) {
      ranked.push({ item, tier, index })
    }
  })
  if (!applies) {
    return null
  }
  // The backend's order (by the server's sort text) breaks ties.
  ranked.sort((a, b) => a.tier - b.tier || a.index - b.index)
  return ranked.map(({ item }) => item)
}
