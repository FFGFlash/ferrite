/**
 * Workspace Trust: asking whether the user trusts the open folder's files.
 * Until they do, language servers and the programs extensions run don't
 * start for it. The dialog opens the first time a folder the user hasn't
 * decided about opens (once a session), when an extension asks, and from
 * the status bar's Restricted Mode item or Manage Workspace Trust.
 */
import { setWorkspaceTrust } from '../ipc/workspace'
import { extensionsStore } from './extensions'
import { reportError } from './notifications'
import { createStore } from './store'
import { workspaceStore } from './workspace'

export interface TrustPrompt {
  /** The name of the extension that asked, if one did. */
  requestedBy: string | null
}

/** The trust dialog, while it's open. */
export const trustPrompt = createStore<TrustPrompt | null>(null)

/** Folders asked about automatically this session. */
const asked = new Set<string>()

/** Opens the trust dialog for the open folder, if there is one. */
export function promptForTrust(requestedBy: string | null = null): void {
  if (workspaceStore.get().info) {
    trustPrompt.set({ requestedBy })
  }
}

/** An extension (by id) asked for the folder to be trusted. */
export function trustRequested(extension: string): void {
  const info = workspaceStore.get().info
  if (!info || info.trust === 'trusted') {
    return
  }
  const name = extensionsStore.get().find(candidate => candidate.id === extension)?.name
  promptForTrust(name ?? extension)
}

export function closeTrustPrompt(): void {
  trustPrompt.set(null)
}

/** Trusts the open folder (or, with `parent`, the folder it's in), or doesn't. */
export async function decideTrust(trusted: boolean, parent = false): Promise<void> {
  closeTrustPrompt()
  try {
    await setWorkspaceTrust(trusted, parent)
  } catch (error) {
    reportError(trusted ? 'Trusting the folder' : 'Restricting the folder', error)
  }
}

// Asks about a folder the user hasn't decided about when it opens.
workspaceStore.subscribe(() => {
  const { info } = workspaceStore.get()
  if (info?.trust === 'unknown' && !asked.has(info.root)) {
    asked.add(info.root)
    promptForTrust()
  } else if (!info) {
    closeTrustPrompt()
  }
})
