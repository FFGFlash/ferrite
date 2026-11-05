; Markdown's inline text: emphasis, code spans, and links. Adapted from the
; queries of tree-sitter-markdown 0.5.3 (MIT, see
; ../../grammars/LICENSE-tree-sitter-markdown), with capture names Flareite
; colors. Flareite's syntax colors have no bold or italic, so emphasis gets a
; color of its own.

(emphasis) @type

(strong_emphasis) @constant

(strikethrough) @comment

(code_span) @string

[
  (emphasis_delimiter)
  (code_span_delimiter)
] @punctuation

[
  (link_text)
  (image_description)
] @function

(link_label) @label

[
  (link_destination)
  (link_title)
  (uri_autolink)
  (email_autolink)
] @string

[
  (backslash_escape)
  (hard_line_break)
  (entity_reference)
  (numeric_character_reference)
] @escape

(image
  [
    "!"
    "["
    "]"
    "("
    ")"
  ] @punctuation)

(inline_link
  [
    "["
    "]"
    "("
    ")"
  ] @punctuation)

(full_reference_link
  [
    "["
    "]"
  ] @punctuation)

(collapsed_reference_link
  [
    "["
    "]"
  ] @punctuation)

(shortcut_link
  [
    "["
    "]"
  ] @punctuation)
