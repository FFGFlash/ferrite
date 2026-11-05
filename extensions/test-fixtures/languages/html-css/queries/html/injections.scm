; Languages embedded in HTML, from tree-sitter-html 0.23.2 (MIT, see
; ../../grammars/LICENSE-tree-sitter-html).
;
; `@injection.content` is the embedded text; `injection.language` names the
; language by id. JavaScript is bundled with Flareite, and CSS comes from this
; extension.

((script_element
  (raw_text) @injection.content)
 (#set! injection.language "javascript"))

((style_element
  (raw_text) @injection.content)
 (#set! injection.language "css"))
