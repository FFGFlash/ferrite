import { afterEach, describe, expect, it } from 'vitest'
import { builtinCommands, keyboardShortcutsText } from './builtin'
import { bindingsOf, registerCommands, setKeybindingOverrides } from './registry'

/** A key combination with `mod` resolved and the modifiers in order, e.g. "ctrl+shift+p". */
function normalizeStep(step: string, mac: boolean): string {
  const parts = step.toLowerCase().split(/\+(?!$)/)
  const key = parts.pop()!
  const modifiers = parts.map(part => (part === 'mod' ? (mac ? 'meta' : 'ctrl') : part)).sort()
  return [...modifiers, key].join('+')
}

describe('built-in keybindings', () => {
  it.each([false, true])('bind each key to one command (mac: %s)', mac => {
    const owners = new Map<string, string[]>()
    for (const command of builtinCommands) {
      for (const binding of bindingsOf(command, mac)) {
        const steps = binding
          .trim()
          .split(/\s+/)
          .map(step => normalizeStep(step, mac))
        const key = steps.join(' ')
        owners.set(key, [...(owners.get(key) ?? []), command.id])
      }
    }
    const shared = [...owners].filter(([, ids]) => ids.length > 1)
    expect(shared).toEqual([])
    // A single key that starts a chord would make the chord unreachable.
    const chordStarts = new Set(
      [...owners.keys()].filter(key => key.includes(' ')).map(key => key.split(' ')[0]),
    )
    expect([...owners.keys()].filter(key => chordStarts.has(key))).toEqual([])
  })
})

describe('keyboard shortcuts list', () => {
  afterEach(() => setKeybindingOverrides({}))

  it('lists each command with its id and current keys', () => {
    const unregister = registerCommands([
      { id: 'test.save', category: 'Test', title: 'Save It', keybinding: 'mod+s', run: () => {} },
      { id: 'test.hidden', title: 'Hidden', hidden: true, run: () => {} },
    ])
    setKeybindingOverrides({ 'test.save': ['mod+s', 'f2'] })
    const text = keyboardShortcutsText(false)
    const row = text.split('\n').find(line => line.startsWith('Test: Save It'))
    expect(row).toMatch(/^Test: Save It\s+test\.save\s+Ctrl\+S, F2$/)
    expect(text).not.toContain('test.hidden')
    expect(text).toContain('[keybindings]')
    unregister()
  })
})
