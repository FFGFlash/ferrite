import { afterEach, describe, expect, it } from 'vitest'
import { keyboardShortcutsText } from './builtin'
import { registerCommands, setKeybindingOverrides } from './registry'

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
