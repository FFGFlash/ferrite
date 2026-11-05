; Languages embedded in Markdown, from tree-sitter-markdown 0.5.3 (MIT, see
; ../../grammars/LICENSE-tree-sitter-markdown).

; Code fences name their language by id (`rust`) or file extension (`rs`).
(fenced_code_block
  (info_string
    (language) @injection.language)
  (code_fence_content) @injection.content)

((html_block) @injection.content
  (#set! injection.language "html"))

((minus_metadata) @injection.content
  (#set! injection.language "yaml"))

((plus_metadata) @injection.content
  (#set! injection.language "toml"))

; The text of paragraphs, headings, and table cells.
((inline) @injection.content
  (#set! injection.language "markdown_inline"))
