import { afterEach, describe, expect, it, vi } from 'vitest'
import type { KeyInput } from '../editor/keymap'
import { fuzzyFilter, fuzzyMatch } from './fuzzy'
import {
  chordStart,
  commandForChord,
  commandForKey,
  executeCommand,
  formatKeybinding,
  matchesKeybinding,
  onKeybindingsChanged,
  primaryKeybinding,
  registerCommands,
  setKeybindingOverrides,
  webviewKeystrokes,
  type Command,
} from './registry'

const key = (k: string, modifiers: Partial<KeyInput> = {}): KeyInput => ({
  key: k,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...modifiers,
})

describe('keybindings', () => {
  it('matches shifted punctuation by its unshifted key', () => {
    // Shift+` reads "~" on a US keyboard.
    const tilde = key('~', { ctrlKey: true, shiftKey: true, code: 'Backquote' })
    expect(matchesKeybinding('ctrl+shift+`', tilde, false)).toBe(true)
    expect(matchesKeybinding('ctrl+`', tilde, false)).toBe(false)
    expect(
      matchesKeybinding(
        'ctrl+shift+1',
        key('!', { ctrlKey: true, shiftKey: true, code: 'Digit1' }),
        false,
      ),
    ).toBe(true)
    // Unshifted keys still match by what they type.
    expect(matchesKeybinding('ctrl+`', key('`', { ctrlKey: true, code: 'Backquote' }), false)).toBe(
      true,
    )
  })

  it('maps mod to Ctrl or Cmd by platform', () => {
    expect(
      matchesKeybinding('mod+shift+p', key('P', { ctrlKey: true, shiftKey: true }), false),
    ).toBe(true)
    expect(
      matchesKeybinding('mod+shift+p', key('P', { metaKey: true, shiftKey: true }), true),
    ).toBe(true)
    expect(
      matchesKeybinding('mod+shift+p', key('P', { ctrlKey: true, shiftKey: true }), true),
    ).toBe(false)
  })

  it('requires exactly the listed modifiers', () => {
    expect(matchesKeybinding('mod+s', key('s', { ctrlKey: true, shiftKey: true }), false)).toBe(
      false,
    )
    expect(matchesKeybinding('f3', key('F3'), false)).toBe(true)
    expect(matchesKeybinding('shift+f3', key('F3', { shiftKey: true }), false)).toBe(true)
    expect(matchesKeybinding('mod+,', key(',', { ctrlKey: true }), false)).toBe(true)
  })

  it('matches and shows arrow keys', () => {
    expect(matchesKeybinding('alt+arrowup', key('ArrowUp', { altKey: true }), false)).toBe(true)
    expect(matchesKeybinding('alt+arrowup', key('ArrowDown', { altKey: true }), false)).toBe(false)
    expect(formatKeybinding('alt+shift+arrowdown', false)).toBe('Alt+Shift+↓')
    expect(formatKeybinding('alt+shift+arrowdown', true)).toBe('⌥⇧↓')
  })

  it('matches Option+letter on macOS by the physical key', () => {
    // Option+Shift+I types "ˆ" on a US Mac keyboard.
    const circumflex = key('ˆ', { altKey: true, shiftKey: true, code: 'KeyI' })
    expect(matchesKeybinding('alt+shift+i', circumflex, true)).toBe(true)
    expect(matchesKeybinding('alt+shift+j', circumflex, true)).toBe(false)
    // Option+Cmd+[ types "“".
    const quote = key('“', { altKey: true, metaKey: true, code: 'BracketLeft' })
    expect(matchesKeybinding('mod+alt+[', quote, true)).toBe(true)
  })

  it('formats chords as their two key combinations', () => {
    expect(formatKeybinding('mod+k mod+0', false)).toBe('Ctrl+K Ctrl+0')
    expect(formatKeybinding('mod+k mod+0', true)).toBe('⌘K ⌘0')
    expect(matchesKeybinding('mod+k mod+0', key('k', { ctrlKey: true }), false)).toBe(false)
  })

  it('formats bindings for display', () => {
    expect(formatKeybinding('mod+shift+p', false)).toBe('Ctrl+Shift+P')
    expect(formatKeybinding('mod+shift+p', true)).toBe('⇧⌘P')
    expect(formatKeybinding('ctrl+pagedown', false)).toBe('Ctrl+PageDown')
  })
})

describe('command registry', () => {
  let unregister = () => {}
  afterEach(() => unregister())

  const command = (overrides: Partial<Command>): Command => ({
    id: 'test.command',
    title: 'Test',
    run: vi.fn(),
    ...overrides,
  })

  it('lists the keystrokes webview pages pass on: single keys and chords’ first keys', () => {
    unregister = registerCommands([
      command({ id: 'a', keybinding: 'mod+p' }),
      command({ id: 'b', keybinding: 'mod+k mod+0' }),
      command({ id: 'c', keybinding: 'mod+shift+p', editorFocus: true }),
      command({ id: 'd', keybinding: 'f1' }),
    ])
    const stroke = (key: string, ctrl = false) => ({
      key,
      ctrl,
      meta: false,
      alt: false,
      shift: false,
    })
    const strokes = webviewKeystrokes(false)
    expect(strokes).toEqual(
      expect.arrayContaining([stroke('p', true), stroke('k', true), stroke('f1')]),
    )
    // Editor-only commands don't apply in a page.
    expect(strokes).not.toContainEqual({ ...stroke('p', true), shift: true })
    const changed = vi.fn()
    const stop = onKeybindingsChanged(changed)
    setKeybindingOverrides({ a: ['mod+o'] })
    expect(changed).toHaveBeenCalled()
    expect(webviewKeystrokes(false)).toContainEqual(stroke('o', true))
    stop()
    setKeybindingOverrides({})
  })

  it('finds enabled commands bound to a key', () => {
    const enabled = command({ id: 'a', keybinding: 'mod+k' })
    const disabled = command({ id: 'b', keybinding: 'mod+j', enabled: () => false })
    const focused = command({ id: 'c', keybinding: 'mod+d', editorFocus: true })
    unregister = registerCommands([enabled, disabled, focused])

    expect(commandForKey(key('k', { ctrlKey: true }), false, false)).toBe(enabled)
    expect(commandForKey(key('j', { ctrlKey: true }), false, false)).toBeUndefined()
    expect(commandForKey(key('d', { ctrlKey: true }), false, false)).toBeUndefined()
    expect(commandForKey(key('d', { ctrlKey: true }), false, true)).toBe(focused)
  })

  it('leaves keys to the shell while a terminal has focus, except terminal commands', () => {
    const close = command({ id: 'close', keybinding: 'mod+w' })
    const palette = command({ id: 'palette', keybinding: 'mod+shift+p', terminal: true })
    unregister = registerCommands([close, palette])

    const ctrlW = key('w', { ctrlKey: true })
    expect(commandForKey(ctrlW, false, false, false)).toBe(close)
    expect(commandForKey(ctrlW, false, false, true)).toBeUndefined()
    const ctrlShiftP = key('P', { ctrlKey: true, shiftKey: true })
    expect(commandForKey(ctrlShiftP, false, false, true)).toBe(palette)
  })

  it('uses platform-specific bindings', () => {
    const replace = command({ keybinding: { default: 'mod+h', mac: 'mod+alt+f' } })
    unregister = registerCommands([replace])
    expect(commandForKey(key('h', { ctrlKey: true }), false, false)).toBe(replace)
    expect(commandForKey(key('f', { metaKey: true, altKey: true }), true, false)).toBe(replace)
    expect(commandForKey(key('h', { metaKey: true }), true, false)).toBeUndefined()
  })

  it('finds commands bound to chords in two steps', () => {
    const foldAll = command({ id: 'foldAll', keybinding: 'mod+k mod+0', editorFocus: true })
    const unfoldAll = command({ id: 'unfoldAll', keybinding: 'mod+k mod+j', editorFocus: true })
    unregister = registerCommands([foldAll, unfoldAll])

    const ctrlK = key('k', { ctrlKey: true })
    expect(commandForKey(ctrlK, false, true)).toBeUndefined()
    expect(chordStart(ctrlK, false, false)).toBeUndefined()
    const first = chordStart(ctrlK, false, true)
    expect(first).toBe('mod+k')
    expect(commandForChord(first!, key('0', { ctrlKey: true }), false, true)).toBe(foldAll)
    expect(commandForChord(first!, key('j', { ctrlKey: true }), false, true)).toBe(unfoldAll)
    expect(commandForChord(first!, key('j'), false, true)).toBeUndefined()
  })

  it('executes commands by id', async () => {
    const run = vi.fn()
    unregister = registerCommands([command({ run })])
    await executeCommand('test.command')
    await executeCommand('missing')
    expect(run).toHaveBeenCalledOnce()
  })
})

describe('fuzzy matching', () => {
  it('matches characters in order and records their positions', () => {
    expect(fuzzyMatch('ofo', 'File: Open Folder')?.indices).toEqual([6, 11, 12])
    expect(fuzzyMatch('xyz', 'File: Open Folder')).toBeNull()
    expect(fuzzyMatch('', 'anything')).toEqual({ score: 0, indices: [] })
  })

  it('ranks word-start and consecutive matches first', () => {
    const items = ['Toggle Side Bar', 'Save', 'Save As']
    const ranked = fuzzyFilter(items, 'sa', item => item).map(entry => entry.item)
    // "Toggle Side Bar" also contains s…a, but scattered matches rank last.
    expect(ranked).toEqual(['Save', 'Save As', 'Toggle Side Bar'])
    expect(fuzzyFilter(items, 'tsb', item => item)[0]?.item).toBe('Toggle Side Bar')
  })
})

describe('keybinding precedence', () => {
  afterEach(() => setKeybindingOverrides({}))

  const command = (id: string, keybinding: string, source?: Command['source']): Command => ({
    id,
    title: id,
    keybinding,
    source,
    run: () => {},
  })
  const ctrlK = key('k', { ctrlKey: true })

  it('prefers built-in commands over extension commands for the same key', () => {
    const unregister = registerCommands([
      command('ext.k', 'mod+k', 'extension'),
      command('builtin.k', 'mod+k'),
    ])
    expect(commandForKey(ctrlK, false, false)?.id).toBe('builtin.k')
    unregister()
  })

  it('lets the user rebind or unbind any command', () => {
    const unregister = registerCommands([
      command('builtin.k', 'mod+k'),
      command('ext.j', 'mod+j', 'extension'),
    ])
    setKeybindingOverrides({ 'ext.j': ['mod+k'], 'builtin.k': [] })
    expect(commandForKey(ctrlK, false, false)?.id).toBe('ext.j')
    expect(commandForKey(key('j', { ctrlKey: true }), false, false)).toBeUndefined()
    expect(primaryKeybinding(command('builtin.k', 'mod+k'), false)).toBeNull()
    unregister()
  })
})
