import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { notifications, notify } from '../state/notifications'
import { reportedProblems } from '../state/problems'
import { Notifications } from './Notifications'

describe('Notifications', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    notifications.set([])
    reportedProblems.set([])
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('hides information after a while, but not while it is hovered', () => {
    render(<Notifications />)
    act(() => {
      notify('info', 'Settings applied.')
    })
    const item = screen.getByRole('status')
    fireEvent.mouseEnter(item)
    act(() => {
      vi.advanceTimersByTime(60_000)
    })
    expect(screen.getByText('Settings applied.')).toBeInTheDocument()

    fireEvent.mouseLeave(item)
    act(() => {
      vi.advanceTimersByTime(10_000)
    })
    expect(screen.queryByText('Settings applied.')).toBeNull()
  })

  it('hides warnings and errors after longer, and lists them in Problems', () => {
    render(<Notifications />)
    act(() => {
      notify('warning', 'Unsaved changes were restored.')
      notify('error', 'Saving failed.')
    })
    act(() => {
      vi.advanceTimersByTime(10_000)
    })
    expect(screen.getByText('Unsaved changes were restored.')).toBeInTheDocument()
    expect(screen.getByRole('alert')).toHaveTextContent('Saving failed.')

    fireEvent.click(screen.getAllByRole('button', { name: 'Dismiss notification' })[0]!)
    expect(screen.queryByText('Unsaved changes were restored.')).toBeNull()
    act(() => {
      vi.advanceTimersByTime(10_000)
    })
    expect(screen.queryByText('Saving failed.')).toBeNull()
    expect(reportedProblems.get().map(problem => [problem.severity, problem.message])).toEqual([
      ['error', 'Saving failed.'],
      ['warning', 'Unsaved changes were restored.'],
    ])
  })

  it('keeps sticky notifications until they are answered', () => {
    render(<Notifications />)
    const allow = vi.fn()
    act(() => {
      notify('info', 'Tidy wants to read documents.', [{ label: 'Allow', run: allow }], {
        sticky: true,
      })
    })
    act(() => {
      vi.advanceTimersByTime(60_000)
    })
    fireEvent.click(screen.getByRole('button', { name: 'Allow' }))
    expect(allow).toHaveBeenCalled()
    expect(screen.queryByText('Tidy wants to read documents.')).toBeNull()
    expect(reportedProblems.get()).toEqual([])
  })
})
