import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderMarkdown } from "./markdown";

const html = (markdown: string) =>
  render(<div>{renderMarkdown(markdown)}</div>).container.innerHTML;

describe("renderMarkdown", () => {
  it("renders code blocks, rules and paragraphs", () => {
    expect(html("```rust\nfn main()\n```\n---\nRuns the\nprogram.")).toBe(
      "<div><pre>fn main()</pre><hr><p>Runs the program.</p></div>",
    );
  });

  it("renders inline code, bold and links as text", () => {
    expect(html("Use `Vec::new` or **see** [docs](https://example.com) \\*now\\*")).toBe(
      "<div><p>Use <code>Vec::new</code> or <strong>see</strong> docs *now*</p></div>",
    );
  });

  it("never interprets HTML", () => {
    expect(html("<img src=x onerror=alert(1)>")).toBe(
      "<div><p>&lt;img src=x onerror=alert(1)&gt;</p></div>",
    );
  });
});
