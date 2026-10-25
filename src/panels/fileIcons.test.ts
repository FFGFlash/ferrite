import { File, FileBraces, FileCode, FileCog, FileImage, FileLock, FileText } from 'lucide-react'
import { describe, expect, it } from 'vitest'
import { fileIcon } from './fileIcons'

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
