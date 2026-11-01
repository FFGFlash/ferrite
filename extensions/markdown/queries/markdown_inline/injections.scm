; Languages embedded in Markdown's inline text, from tree-sitter-markdown
; 0.5.3 (MIT, see ../../grammars/LICENSE-tree-sitter-markdown).

((html_tag) @injection.content
  (#set! injection.language "html"))

((latex_block) @injection.content
  (#set! injection.language "latex"))
