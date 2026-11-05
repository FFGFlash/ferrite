; Languages embedded in SVG: CSS in <style> and JavaScript in <script>, as
; text or in a CDATA section.
;
; `@injection.content` is the embedded text; `injection.language` names the
; language by id. CSS comes from this extension, and JavaScript from the
; JavaScript extension.

((element
  (STag (Name) @_name)
  (content (CharData) @injection.content))
 (#eq? @_name "style")
 (#set! injection.language "css"))

((element
  (STag (Name) @_name)
  (content (CDSect (CData) @injection.content)))
 (#eq? @_name "style")
 (#set! injection.language "css"))

((element
  (STag (Name) @_name)
  (content (CharData) @injection.content))
 (#eq? @_name "script")
 (#set! injection.language "javascript"))

((element
  (STag (Name) @_name)
  (content (CDSect (CData) @injection.content)))
 (#eq? @_name "script")
 (#set! injection.language "javascript"))
