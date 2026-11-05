import { ShieldAlert, ShieldCheck } from 'lucide-react'
import { useState } from 'react'
import { useStore } from '../state/store'
import { closeTrustPrompt, decideTrust, trustPrompt } from '../state/trust'
import { parentOf, workspaceStore } from '../state/workspace'
import { ModalDialog } from './ModalDialog'
import './TrustDialog.css'

/**
 * Asks whether the user trusts the open folder's files. Closing it without
 * answering leaves the folder restricted, and it asks again next session.
 */
export function TrustDialog() {
  const prompt = useStore(trustPrompt)
  const info = useStore(workspaceStore).info
  if (!prompt || !info) {
    return null
  }
  return (
    <Dialog
      key={info.root}
      root={info.root}
      trusted={info.trust === 'trusted'}
      requestedBy={prompt.requestedBy}
    />
  )
}

interface DialogProps {
  root: string
  trusted: boolean
  requestedBy: string | null
}

function Dialog({ root, trusted, requestedBy }: DialogProps) {
  const [parent, setParent] = useState(false)
  const parentFolder = parentOf(root)
  const button = (label: string, run: () => void, primary = false) => (
    <button
      type="button"
      className={`dialog__button${primary ? ' dialog__button--primary' : ''}`}
      onClick={run}
    >
      {label}
    </button>
  )

  return (
    <ModalDialog
      title={
        trusted
          ? 'You trust the files in this folder'
          : 'Do you trust the authors of the files in this folder?'
      }
      icon={
        trusted ? (
          <ShieldCheck size={20} aria-hidden="true" />
        ) : (
          <ShieldAlert size={20} aria-hidden="true" />
        )
      }
      onClose={closeTrustPrompt}
      extra={
        !trusted &&
        parentFolder !== root && (
          <label className="trust-dialog__parent">
            <input
              type="checkbox"
              checked={parent}
              onChange={event => setParent(event.target.checked)}
            />
            Trust every folder in {parentFolder}
          </label>
        )
      }
      buttons={
        trusted ? (
          <>
            {button('Don’t Trust', () => void decideTrust(false))}
            {button('Close', closeTrustPrompt, true)}
          </>
        ) : (
          <>
            {button('Don’t Trust', () => void decideTrust(false))}
            {button(
              parent ? 'Trust Folders' : 'Trust Folder',
              () => void decideTrust(true, parent),
              true,
            )}
          </>
        )
      }
    >
      <p className="trust-dialog__folder">{root}</p>
      {requestedBy && !trusted && (
        <p>
          <strong>{requestedBy}</strong> needs the folder to be trusted to work.
        </p>
      )}
      <p>
        Language servers, and programs extensions run (such as Git), can run code from the folder:
        build scripts, hooks, and the commands its settings name.{' '}
        {trusted
          ? 'They run, as you trust it.'
          : 'Until you trust it, Flareite doesn’t run them here. Editing and everything else work.'}
      </p>
      {!trusted && <p>Trust only folders you got from people you trust.</p>}
    </ModalDialog>
  )
}
