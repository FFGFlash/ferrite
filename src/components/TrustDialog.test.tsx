import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { WorkspaceInfo } from '../ipc/workspace'

vi.mock('../ipc/workspace', () => ({
  listDirectory: vi.fn(async () => []),
  setWorkspaceTrust: vi.fn(async () => {}),
}))

const ipc = await import('../ipc/workspace')
const { setWorkspace, setWorkspaceTrustInfo } = await import('../state/workspace')
const { closeTrustPrompt, trustPrompt, trustRequested } = await import('../state/trust')
const { extensionsStore } = await import('../state/extensions')
const { TrustDialog } = await import('./TrustDialog')

const folder = (root: string, trust: WorkspaceInfo['trust']): WorkspaceInfo => ({
  root,
  name: root.split('/').pop() ?? root,
  trust,
})

describe('TrustDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    act(() => {
      closeTrustPrompt()
      setWorkspace(null)
    })
  })

  it('asks once a session about a folder the user has not decided about', async () => {
    const user = userEvent.setup()
    render(<TrustDialog />)
    act(() => setWorkspace(folder('/code/app', 'unknown')))
    const dialog = screen.getByRole('alertdialog', {
      name: 'Do you trust the authors of the files in this folder?',
    })
    expect(dialog).toHaveTextContent('/code/app')
    expect(dialog).toHaveFocus()

    // Escape leaves it restricted, without deciding.
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('alertdialog')).toBeNull()
    expect(ipc.setWorkspaceTrust).not.toHaveBeenCalled()
    act(() => setWorkspace(folder('/code/app', 'unknown')))
    expect(screen.queryByRole('alertdialog')).toBeNull()
  })

  it('trusts the folder, or every folder in its parent', async () => {
    const user = userEvent.setup()
    render(<TrustDialog />)
    act(() => setWorkspace(folder('/code/lib', 'unknown')))
    await user.click(screen.getByRole('button', { name: 'Trust Folder' }))
    expect(ipc.setWorkspaceTrust).toHaveBeenCalledWith(true, false)
    expect(screen.queryByRole('alertdialog')).toBeNull()

    act(() => setWorkspace(folder('/code/tool', 'unknown')))
    await user.click(screen.getByRole('checkbox', { name: 'Trust every folder in /code' }))
    await user.click(screen.getByRole('button', { name: 'Trust Folders' }))
    expect(ipc.setWorkspaceTrust).toHaveBeenLastCalledWith(true, true)
  })

  it('names the extension that asks, and can take trust back', async () => {
    const user = userEvent.setup()
    render(<TrustDialog />)
    extensionsStore.set([{ id: 'flareite.git', name: 'Git' } as never])
    act(() => setWorkspace(folder('/code/repo', 'untrusted')))
    expect(screen.queryByRole('alertdialog')).toBeNull()
    act(() => trustRequested('flareite.git'))
    expect(screen.getByRole('alertdialog')).toHaveTextContent(
      'Git needs the folder to be trusted to work.',
    )
    await user.click(screen.getByRole('button', { name: 'Don’t Trust' }))
    expect(ipc.setWorkspaceTrust).toHaveBeenCalledWith(false, false)

    // A trusted folder says so, and requests are ignored.
    act(() => setWorkspaceTrustInfo(folder('/code/repo', 'trusted')))
    act(() => trustRequested('flareite.git'))
    expect(trustPrompt.get()).toBeNull()
    act(() => trustPrompt.set({ requestedBy: null }))
    expect(
      screen.getByRole('alertdialog', { name: 'You trust the files in this folder' }),
    ).toBeInTheDocument()
    extensionsStore.set([])
  })
})
