; Markdown's block structure. Adapted from the queries of
; tree-sitter-markdown 0.5.3 (MIT, see
; ../../grammars/LICENSE-tree-sitter-markdown), with capture names Flareite
; colors. Text inside blocks (emphasis, code spans, links) is highlighted by
; the markdown_inline language (see injections.scm), painted over this.

; A heading's whole line, so the `#` marks and the text share a color.
[
  (atx_heading)
  (setext_heading)
] @keyword

[
  (list_marker_plus)
  (list_marker_minus)
  (list_marker_star)
  (list_marker_dot)
  (list_marker_parenthesis)
  (thematic_break)
  (block_quote_marker)
  (block_continuation)
  (fenced_code_block_delimiter)
  (pipe_table_delimiter_row)
  "|"
] @punctuation

[
  (task_list_marker_checked)
  (task_list_marker_unchecked)
] @constant

; The language after a code fence. The code itself is highlighted by that
; language, if one is loaded.
(info_string) @label

(indented_code_block) @string

(link_reference_definition
  (link_label) @label)

[
  (link_destination)
  (link_title)
] @string

[
  (backslash_escape)
  (entity_reference)
  (numeric_character_reference)
] @escape

; YAML front matter. (TOML front matter is highlighted as TOML.)
(minus_metadata) @comment
