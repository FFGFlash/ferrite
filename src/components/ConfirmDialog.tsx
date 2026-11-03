import { TriangleAlert } from 'lucide-react'
import { answerConfirm, confirmPrompt } from '../state/prompts'
import { useStore } from '../state/store'
import { ModalDialog } from './ModalDialog'

/** A confirmation an extension asks for, such as before discarding changes. */
export function ConfirmDialog() {
  const prompt = useStore(confirmPrompt)
  if (!prompt) {
    return null
  }
  const [main, ...others] = prompt.buttons
  return (
    <ModalDialog
      key={prompt.id}
      title={prompt.message}
      icon={prompt.destructive && <TriangleAlert size={20} aria-hidden="true" />}
      onClose={() => answerConfirm(prompt, null)}
      buttons={
        <>
          <button
            type="button"
            className="dialog__button"
            onClick={() => answerConfirm(prompt, null)}
          >
            Cancel
          </button>
          {others.map((label, index) => (
            <button
              key={label}
              type="button"
              className="dialog__button"
              onClick={() => answerConfirm(prompt, index + 1)}
            >
              {label}
            </button>
          ))}
          <button
            type="button"
            className={`dialog__button ${prompt.destructive ? 'dialog__button--danger' : 'dialog__button--primary'}`}
            onClick={() => answerConfirm(prompt, 0)}
          >
            {main}
          </button>
        </>
      }
    >
      {prompt.detail?.split(/\n\s*\n/).map((paragraph, index) => (
        <p key={index}>{paragraph}</p>
      ))}
      <p className="dialog__source">{prompt.extension}</p>
    </ModalDialog>
  )
}
