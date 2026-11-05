; HTML highlighting, from tree-sitter-html 0.23.2 (MIT, see
; ../../grammars/LICENSE-tree-sitter-html), plus `=` and entities.
;
; Ferrite colors a capture by its first segment: @tag, @attribute, @string,
; @comment, @constant, @punctuation, @operator, @escape, and so on (see
; docs/extensions.md). Captures it doesn't know are left uncolored.

(tag_name) @tag
(erroneous_end_tag_name) @tag.error
(doctype) @constant
(attribute_name) @attribute
(attribute_value) @string
(quoted_attribute_value) @string
(entity) @escape
(comment) @comment

"=" @operator

[
  "<"
  ">"
  "</"
  "/>"
] @punctuation.bracket
