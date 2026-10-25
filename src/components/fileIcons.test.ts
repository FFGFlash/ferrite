import {
  File,
  FileBraces,
  FileCode,
  FileCog,
  FileImage,
  FileLock,
  FileText,
  Folder,
  FolderOpen,
} from 'lucide-react'
import { describe, expect, it } from 'vitest'
import type { Associations, LoadedIconTheme } from '../state/iconTheme'
import { entryIcon, fileIcon } from './fileIcons'

const associations = (partial: Partial<Associations> = {}): Associations => ({
  file: null,
  folder: null,
  folderExpanded: null,
  fileExtensions: new Map(),
  fileNames: new Map(),
  folderNames: new Map(),
  folderNamesExpanded: new Map(),
  ...partial,
})

const ids = [
  'file',
  'folder',
  'folder-open',
  'rust',
  'ts',
  'dts',
  'cargo',
  'src',
  'src-open',
  'pale',
]
const theme = (
  dark: Partial<Associations>,
  light: Partial<Associations> = {},
): LoadedIconTheme => ({
  id: 'example.icons.test',
  icons: new Map(ids.map(id => [id, `data:${id}`])),
  associations: associations(dark),
  light: associations(light),
})

const file = (name: string, icons: LoadedIconTheme | null, light = false) =>
  entryIcon(name, null, icons, light)
const folder = (name: string, expanded: boolean, icons: LoadedIconTheme | null, light = false) =>
  entryIcon(name, { expanded }, icons, light)

describe('fileIcon', () => {
  it('picks icons by extension, ignoring case', () => {
    expect(fileIcon('main.rs')).toBe(FileCode)
    expect(fileIcon('App.TSX')).toBe(FileCode)
    expect(fileIcon('tsconfig.json')).toBe(FileBraces)
    expect(fileIcon('logo.svg')).toBe(FileImage)
    expect(fileIcon('Cargo.lock')).toBe(FileLock)
  })

  it('recognizes well-known names, with or without a suffix', () => {
    expect(fileIcon('pnpm-lock.yaml')).toBe(FileLock)
    expect(fileIcon('.gitignore')).toBe(FileCog)
    expect(fileIcon('.env.local')).toBe(FileCog)
    expect(fileIcon('LICENSE-MIT')).toBe(FileText)
    expect(fileIcon('Dockerfile')).toBe(FileCog)
  })

  it('falls back to a plain file', () => {
    expect(fileIcon('data.xyz')).toBe(File)
    expect(fileIcon('constructor')).toBe(File)
    expect(fileIcon('')).toBe(File)
  })
})

describe('entryIcon', () => {
  const colorful = theme(
    {
      fileExtensions: new Map([
        ['rs', 'rust'],
        ['ts', 'ts'],
        ['d.ts', 'dts'],
      ]),
      fileNames: new Map([['cargo.toml', 'cargo']]),
      folderNames: new Map([['src', 'src']]),
      folderNamesExpanded: new Map([['src', 'src-open']]),
    },
    { fileExtensions: new Map([['rs', 'pale']]) },
  )

  it('uses built-in icons without a theme', () => {
    expect(file('main.rs', null)).toEqual({ Icon: FileCode })
    expect(folder('src', false, null)).toEqual({ Icon: Folder })
    expect(folder('src', true, null)).toEqual({ Icon: FolderOpen })
  })

  it('matches file names first, then the longest extension, ignoring case', () => {
    expect(file('Cargo.TOML', colorful)).toEqual({ src: 'data:cargo' })
    expect(file('lib.RS', colorful)).toEqual({ src: 'data:rust' })
    expect(file('index.d.ts', colorful)).toEqual({ src: 'data:dts' })
    expect(file('index.ts', colorful)).toEqual({ src: 'data:ts' })
    // A leading dot starts the name rather than an extension.
    expect(file('.ts', colorful)).toEqual({ Icon: File })
  })

  it('matches folder names, open and closed', () => {
    expect(folder('SRC', false, colorful)).toEqual({ src: 'data:src' })
    expect(folder('src', true, colorful)).toEqual({ src: 'data:src-open' })
    const closedOnly = theme({ folderNames: new Map([['src', 'src']]) })
    expect(folder('src', true, closedOnly)).toEqual({ src: 'data:src' })
  })

  it('looks in the light section first with a light color theme', () => {
    expect(file('main.rs', colorful, true)).toEqual({ src: 'data:pale' })
    expect(file('main.ts', colorful, true)).toEqual({ src: 'data:ts' })
    expect(file('main.rs', colorful, false)).toEqual({ src: 'data:rust' })
  })

  it("falls back to the theme's defaults, then to the built-in icons", () => {
    expect(file('tsconfig.json', colorful)).toEqual({ Icon: FileBraces })
    expect(folder('docs', true, colorful)).toEqual({ Icon: FolderOpen })

    const complete = theme({ file: 'file', folder: 'folder', folderExpanded: 'folder-open' })
    expect(file('tsconfig.json', complete)).toEqual({ src: 'data:file' })
    expect(folder('docs', false, complete)).toEqual({ src: 'data:folder' })
    expect(folder('docs', true, complete)).toEqual({ src: 'data:folder-open' })
    const closedDefault = theme({ folder: 'folder' })
    expect(folder('docs', true, closedDefault)).toEqual({ src: 'data:folder' })
  })

  it('ignores associations whose icon is missing', () => {
    const broken = theme({ fileExtensions: new Map([['rs', 'gone']]) })
    expect(file('main.rs', broken)).toEqual({ Icon: FileCode })
  })
})
