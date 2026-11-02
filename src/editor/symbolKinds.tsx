/**
 * Symbol and completion kinds (`function`, `struct`, …, as the backend names
 * them): their icons, and their names as VS Code shows them.
 */
import './symbolKinds.css'

/** A kind's glyph, colored by kind. */
export function SymbolKindIcon({ kind }: { kind: string | null }) {
  return (
    <span className="symbol-kind" data-kind={kind ?? 'text'} aria-hidden="true">
      {glyph(kind)}
    </span>
  )
}

function glyph(kind: string | null): string {
  switch (kind) {
    case 'function':
    case 'method':
    case 'constructor':
      return 'ƒ'
    case 'variable':
      return 'x'
    case 'field':
    case 'property':
      return '◆'
    case 'struct':
    case 'class':
    case 'interface':
    case 'enum':
    case 'typeParameter':
      return 'T'
    case 'module':
    case 'namespace':
    case 'package':
    case 'file':
    case 'folder':
      return '{}'
    case 'keyword':
      return 'k'
    case 'constant':
    case 'enumMember':
      return 'c'
    case 'snippet':
      return '…'
    default:
      return '·'
  }
}

/** Singular and plural names of symbol kinds. */
const NAMES = new Map<string, [string, string]>([
  ['file', ['file', 'files']],
  ['module', ['module', 'modules']],
  ['namespace', ['namespace', 'namespaces']],
  ['package', ['package', 'packages']],
  ['class', ['class', 'classes']],
  ['method', ['method', 'methods']],
  ['property', ['property', 'properties']],
  ['field', ['field', 'fields']],
  ['constructor', ['constructor', 'constructors']],
  ['enum', ['enumeration', 'enumerations']],
  ['interface', ['interface', 'interfaces']],
  ['function', ['function', 'functions']],
  ['variable', ['variable', 'variables']],
  ['constant', ['constant', 'constants']],
  ['string', ['string', 'strings']],
  ['number', ['number', 'numbers']],
  ['boolean', ['boolean', 'booleans']],
  ['array', ['array', 'arrays']],
  ['object', ['object', 'objects']],
  ['key', ['key', 'keys']],
  ['null', ['null', 'nulls']],
  ['enumMember', ['enumeration member', 'enumeration members']],
  ['struct', ['struct', 'structs']],
  ['event', ['event', 'events']],
  ['operator', ['operator', 'operators']],
  ['typeParameter', ['type parameter', 'type parameters']],
])

/** A symbol kind's name, such as "enumeration member". */
export function symbolKindName(kind: string): string {
  return (NAMES.get(kind) ?? NAMES.get('property')!)[0]
}

/** The heading for symbols of a kind, such as "functions (3)". */
export function symbolKindGroup(kind: string, count: number): string {
  return `${(NAMES.get(kind) ?? NAMES.get('property')!)[1]} (${count})`
}
