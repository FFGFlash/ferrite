import { afterEach, describe, expect, it, vi } from 'vitest'
import { getAppInfo, log } from './index'

// Tests run outside the desktop shell, exercising the browser fallbacks.
describe('ipc outside the desktop shell', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('reports no application info', async () => {
    await expect(getAppInfo()).resolves.toBeNull()
  })

  it('writes log messages to the console', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => {})

    log('warn', 'careful')
    log('trace', 'details')

    expect(warn).toHaveBeenCalledWith('careful')
    expect(debug).toHaveBeenCalledWith('details')
  })
})
