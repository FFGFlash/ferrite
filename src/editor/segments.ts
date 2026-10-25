import type { CSSProperties } from "react";
import { TOKEN_KINDS } from "../ipc/editor";

/** A run of a line's text and how it is drawn (no class and no style for plain text). */
export interface Segment {
  start: number;
  end: number;
  className: string;
  style?: CSSProperties;
}

/** A range of a line drawn on top of the token colors: a class, a style, or both. */
export interface TextOverlay {
  from: number;
  to: number;
  className?: string;
  style?: CSSProperties;
}

/** Class of the symbol under the pointer while the go-to-definition modifier is held. */
export const LINK_CLASS = "editor-view__link";

/**
 * `text` split into runs by its highlight spans (flat `[start, end, kind]`
 * triples) and by `overlays` (the go-to-definition link, extension
 * decorations), which add their classes and styles to the runs they cover.
 * Runs cover the whole text, in order.
 */
export function segments(
  text: string,
  spans: readonly number[],
  overlays: readonly TextOverlay[] = [],
): Segment[] {
  const base: Segment[] = [];
  const push = (start: number, end: number, className: string) => {
    if (start < end) {
      base.push({ start, end, className });
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
  if (overlays.length === 0) {
    return base;
  }

  const cuts = new Set<number>();
  for (const overlay of overlays) {
    cuts.add(Math.max(0, overlay.from));
    cuts.add(Math.min(text.length, overlay.to));
  }
  const out: Segment[] = [];
  for (const segment of base) {
    const points = [segment.start, segment.end];
    for (const cut of cuts) {
      if (cut > segment.start && cut < segment.end) {
        points.push(cut);
      }
    }
    points.sort((a, b) => a - b);
    for (let i = 0; i + 1 < points.length; i++) {
      const start = points[i]!;
      const end = points[i + 1]!;
      const classes = segment.className ? [segment.className] : [];
      let style: CSSProperties | undefined;
      for (const overlay of overlays) {
        if (overlay.from <= start && end <= overlay.to) {
          if (overlay.className) {
            classes.push(overlay.className);
          }
          if (overlay.style) {
            style = { ...style, ...overlay.style };
          }
        }
      }
      out.push(
        style
          ? { start, end, className: classes.join(" "), style }
          : { start, end, className: classes.join(" ") },
      );
    }
  }
  return out;
}
