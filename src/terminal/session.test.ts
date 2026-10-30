import { afterEach, describe, expect, it, vi } from 'vitest'
import { isClipboardKey, terminalTheme } from './session'

const key = (key: string, modifiers: Partial<KeyboardEvent> = {}) =>
  ({
    type: 'keydown',
    key,
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    ...modifiers,
  }) as KeyboardEvent

function onPlatform(userAgent: string) {
  vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue(userAgent)
}

describe('isClipboardKey', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('pastes with Ctrl+V and copies a selection with Ctrl+C on Windows', () => {
    onPlatform('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')
    expect(isClipboardKey(key('v', { ctrlKey: true }), false, false)).toBe(true)
    expect(isClipboardKey(key('c', { ctrlKey: true }), true, false)).toBe(true)
    // Without a selection, Ctrl+C interrupts.
    expect(isClipboardKey(key('c', { ctrlKey: true }), false, false)).toBe(false)
    expect(isClipboardKey(key('C', { ctrlKey: true, shiftKey: true }), false, false)).toBe(true)
  })

  it('leaves Ctrl+C and Ctrl+V to the shell on Linux', () => {
    onPlatform('Mozilla/5.0 (X11; Linux x86_64)')
    expect(isClipboardKey(key('v', { ctrlKey: true }), false, false)).toBe(false)
    expect(isClipboardKey(key('c', { ctrlKey: true }), true, false)).toBe(false)
    expect(isClipboardKey(key('V', { ctrlKey: true, shiftKey: true }), false, false)).toBe(true)
    expect(isClipboardKey(key('C', { ctrlKey: true, shiftKey: true }), true, false)).toBe(true)
  })

  it('uses Cmd on macOS', () => {
    expect(isClipboardKey(key('v', { metaKey: true }), false, true)).toBe(true)
    expect(isClipboardKey(key('c', { metaKey: true }), true, true)).toBe(true)
    expect(isClipboardKey(key('c', { ctrlKey: true }), true, true)).toBe(false)
  })

  it('ignores other keys and events', () => {
    expect(isClipboardKey(key('v', { ctrlKey: true, altKey: true }), false, false)).toBe(false)
    expect(isClipboardKey({ ...key('v', { ctrlKey: true }), type: 'keyup' }, false, false)).toBe(
      false,
    )
    expect(isClipboardKey(key('x', { ctrlKey: true }), true, false)).toBe(false)
  })
})

describe('terminalTheme', () => {
  it('reads colors from the terminal tokens', () => {
    const tokens: Record<string, string> = {
      '--color-terminal-background': ' #101010',
      '--color-terminal-foreground': '#eeeeee',
      '--color-terminal-selection': 'rgb(0 0 255 / 0.3)',
      '--color-terminal-ansi-red': '#ff0000',
      '--color-terminal-ansi-bright-cyan': '#00ffff',
    }
    const style = { getPropertyValue: (name: string) => tokens[name] ?? '' }
    const theme = terminalTheme(style as CSSStyleDeclaration)
    expect(theme.background).toBe('#101010')
    expect(theme.cursorAccent).toBe('#101010')
    expect(theme.foreground).toBe('#eeeeee')
    expect(theme.selectionBackground).toBe('rgb(0 0 255 / 0.3)')
    expect(theme.red).toBe('#ff0000')
    expect(theme.brightCyan).toBe('#00ffff')
    // Unset tokens leave xterm.js's defaults.
    expect(theme.blue).toBeUndefined()
  })
})
