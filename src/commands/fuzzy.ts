/**
 * Fuzzy matching for short lists (commands). Every query character must
 * appear in order; matches at word starts and consecutive runs score higher.
 * Workspace files are matched in the backend instead.
 */
export interface FuzzyMatch {
  score: number;
  /** Indices of the matched characters in the text. */
  indices: number[];
}

export function fuzzyMatch(query: string, text: string): FuzzyMatch | null {
  const needle = query.toLowerCase().replace(/\s+/g, "");
  if (!needle) {
    return { score: 0, indices: [] };
  }
  const haystack = text.toLowerCase();
  const indices: number[] = [];
  let score = 0;
  let from = 0;
  for (const char of needle) {
    const index = haystack.indexOf(char, from);
    if (index === -1) {
      return null;
    }
    const previous = indices.at(-1);
    if (previous !== undefined && index === previous + 1) {
      score += 5;
    }
    if (index === 0 || /[\s:._\-/]/.test(text[index - 1] ?? "")) {
      score += 10;
    }
    score -= index - from;
    indices.push(index);
    from = index + 1;
  }
  return { score, indices };
}

/** Items matching `query`, best first; ties keep their original order. */
export function fuzzyFilter<T>(
  items: readonly T[],
  query: string,
  text: (item: T) => string,
): { item: T; match: FuzzyMatch }[] {
  return items
    .map((item, order) => ({ item, order, match: fuzzyMatch(query, text(item)) }))
    .filter((entry): entry is { item: T; order: number; match: FuzzyMatch } => entry.match !== null)
    .sort((a, b) => b.match.score - a.match.score || a.order - b.order)
    .map(({ item, match }) => ({ item, match }));
}
