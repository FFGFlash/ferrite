import { TOKEN_KINDS } from "../ipc/editor";

/** A run of a line's text and the classes it is drawn with (empty for plain text). */
export interface Segment {
  start: number;
  end: number;
  className: string;
}

/** Class of the symbol under the pointer while the go-to-definition modifier is held. */
export const LINK_CLASS = "editor-view__link";

/**
 * `text` split into runs by its highlight spans (flat `[start, end, kind]`
 * triples) and by `link`, a `[from, to)` range drawn as a link on top of
 * the token colors. Runs cover the whole text, in order.
 */
export function segments(
  text: string,
  spans: readonly number[],
  link: readonly [number, number] | null,
): Segment[] {
  const out: Segment[] = [];
  const push = (start: number, end: number, className: string) => {
    if (start >= end) {
      return;
    }
    if (!link || end <= link[0] || start >= link[1]) {
      out.push({ start, end, className });
      return;
    }
    const linked = className ? `${className} ${LINK_CLASS}` : LINK_CLASS;
    const from = Math.max(start, link[0]);
    const to = Math.min(end, link[1]);
    if (start < from) {
      out.push({ start, end: from, className });
    }
    out.push({ start: from, end: to, className: linked });
    if (to < end) {
      out.push({ start: to, end, className });
    }
  };
  let at = 0;
  for (let i = 0; i + 2 < spans.length; i += 3) {
    const start = spans[i]!;
    const end = Math.min(spans[i + 1]!, text.length);
    if (start < at || start >= end) {
      continue;
    }
    push(at, start, "");
    push(start, end, `tok-${TOKEN_KINDS[spans[i + 2]! - 1] ?? "none"}`);
    at = end;
  }
  push(at, text.length, "");
  return out;
}
