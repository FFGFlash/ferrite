; SVG highlighting with the XML grammar, from tree-sitter-xml 0.7.0 (MIT, see
; ../../grammars/LICENSE-tree-sitter-xml). Elements and attributes use the
; same captures as HTML's, so an .svg file looks like SVG inline in HTML.
;
; Ferrite colors a capture by its first segment: @tag, @attribute, @string,
; @comment, @constant, @punctuation, @operator, @escape, and so on (see
; docs/extensions.md). Captures it doesn't know are left uncolored.

; The XML declaration and processing instructions.

"xml" @keyword
[ "version" "encoding" "standalone" ] @attribute
(EncName) @string
(VersionNum) @string
[ "yes" "no" ] @string

(PI (PITarget) @keyword)
(XmlModelPI "xml-model" @keyword)
(StyleSheetPI "xml-stylesheet" @keyword)
(PseudoAtt (Name) @attribute)
(PseudoAtt (PseudoAttValue) @string)

; The doctype, and declarations in it.

(doctypedecl "DOCTYPE" @keyword)
(doctypedecl (Name) @type)
[ "PUBLIC" "SYSTEM" ] @keyword
(PubidLiteral) @string
(SystemLiteral) @string

(elementdecl "ELEMENT" @keyword (Name) @tag)
(AttlistDecl "ATTLIST" @keyword (Name) @tag)
(AttDef (Name) @attribute)
(GEDecl "ENTITY" @keyword (Name) @constant)
(GEDecl (EntityValue) @string)
(PEDecl "ENTITY" @keyword (Name) @constant)
(PEDecl (EntityValue) @string)

; Elements.

(STag (Name) @tag)
(ETag (Name) @tag)
(EmptyElemTag (Name) @tag)
(Attribute (Name) @attribute)
(Attribute (AttValue) @string)

[ (EntityRef) (CharRef) ] @escape
(PEReference) @constant
(Comment) @comment

"=" @operator

[
  "<"
  ">"
  "</"
  "/>"
  "<?"
  "?>"
  "<!"
  "]]>"
] @punctuation.bracket

(CDStart) @punctuation.bracket
