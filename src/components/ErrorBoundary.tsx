import { Component, type ErrorInfo, type ReactNode } from 'react'
import { log } from '../ipc'
import './ErrorBoundary.css'

interface ErrorBoundaryProps {
  /** What failed, as shown to the user: "The editor", "The Explorer view". */
  name: string
  /** A change of this value (e.g. another document) clears the error. */
  resetKey?: unknown
  children: ReactNode
}

interface ErrorBoundaryState {
  error: Error | null
}

/**
 * Contains a rendering error to the part of the workbench that failed, so
 * the rest stays usable. The error is logged, and the user can try the part
 * again or reload the window (the backend keeps documents and their unsaved
 * changes, so reloading loses nothing).
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  override state: ErrorBoundaryState = { error: null }

  static getDerivedStateFromError(error: unknown): ErrorBoundaryState {
    return { error: error instanceof Error ? error : new Error(String(error)) }
  }

  override componentDidCatch(error: unknown, info: ErrorInfo): void {
    const detail = error instanceof Error ? (error.stack ?? error.message) : String(error)
    log('error', `${this.props.name} failed to render: ${detail}${info.componentStack ?? ''}`)
  }

  override componentDidUpdate(previous: ErrorBoundaryProps): void {
    if (this.state.error && previous.resetKey !== this.props.resetKey) {
      this.setState({ error: null })
    }
  }

  override render(): ReactNode {
    const { error } = this.state
    if (!error) {
      return this.props.children
    }
    return (
      <div className="error-boundary" role="alert">
        <p className="error-boundary__title">
          {this.props.name} stopped working because of an internal error.
        </p>
        <p className="error-boundary__detail">{error.message}</p>
        <div className="error-boundary__actions">
          <button
            type="button"
            className="error-boundary__button"
            onClick={() => this.setState({ error: null })}
          >
            Try Again
          </button>
          <button
            type="button"
            className="error-boundary__button error-boundary__button--secondary"
            onClick={() => window.location.reload()}
          >
            Reload Window
          </button>
        </div>
      </div>
    )
  }
}
