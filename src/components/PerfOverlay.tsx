import { useEffect, useState } from 'react'
import { getPerfStats, type PerfStats } from '../ipc/perf'
import { extensionsStore, useExtensionUsage } from '../state/extensions'
import { perfSnapshot, type SeriesSummary } from '../state/perf'
import { createStore, useStore } from '../state/store'
import './PerfOverlay.css'

const REFRESH_MS = 1000

export const perfOverlayStore = createStore(false)

export const togglePerfOverlay = () => perfOverlayStore.update(open => !open)

const ms = (us: number) => (us / 1000).toFixed(us < 10_000 ? 2 : 0)
const mb = (bytes: number) => `${(bytes / (1024 * 1024)).toFixed(1)} MB`

/** Live timing and memory statistics, toggled with "Developer: Toggle Performance Overlay". */
export function PerfOverlay() {
  const open = useStore(perfOverlayStore)
  return open ? <PerfOverlayContent /> : null
}

function PerfOverlayContent() {
  const [frontend, setFrontend] = useState<SeriesSummary[]>([])
  const [backend, setBackend] = useState<PerfStats | null>(null)

  const extensions = useStore(extensionsStore)
  const usage = useExtensionUsage(REFRESH_MS)

  useEffect(() => {
    const refresh = () => {
      setFrontend(perfSnapshot())
      getPerfStats()
        .then(setBackend)
        .catch(() => setBackend(null))
    }
    refresh()
    const timer = setInterval(refresh, REFRESH_MS)
    return () => clearInterval(timer)
  }, [])

  const running = extensions.filter(extension => (usage.get(extension.id)?.calls ?? 0) > 0)
  const heap = (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory
  return (
    <aside className="perf-overlay" aria-label="Performance statistics">
      <header className="perf-overlay__header">
        <span>Performance</span>
        <button
          type="button"
          className="perf-overlay__close"
          aria-label="Close performance statistics"
          onClick={() => perfOverlayStore.set(false)}
        >
          ×
        </button>
      </header>
      <table className="perf-overlay__table">
        <thead>
          <tr>
            <th>Frontend</th>
            <th>n</th>
            <th>p50</th>
            <th>p95</th>
            <th>max</th>
          </tr>
        </thead>
        <tbody>
          {frontend.map(entry => (
            <tr key={entry.name}>
              <td>{entry.name}</td>
              <td>{entry.count}</td>
              <td>{format(entry.p50, entry.unit)}</td>
              <td>{format(entry.p95, entry.unit)}</td>
              <td>{format(entry.max, entry.unit)}</td>
            </tr>
          ))}
        </tbody>
        <thead>
          <tr>
            <th>Backend (ms)</th>
            <th>n</th>
            <th>p50</th>
            <th>p95</th>
            <th>max</th>
          </tr>
        </thead>
        <tbody>
          {backend?.timings.map(timing => (
            <tr key={timing.name}>
              <td>{timing.name}</td>
              <td>{timing.count}</td>
              <td>{ms(timing.p50Us)}</td>
              <td>{ms(timing.p95Us)}</td>
              <td>{ms(timing.maxUs)}</td>
            </tr>
          ))}
        </tbody>
        <thead>
          <tr>
            <th>Tasks (ms)</th>
            <th>done</th>
            <th>run p50</th>
            <th>run p95</th>
            <th>wait p95</th>
          </tr>
        </thead>
        <tbody>
          {backend?.tasks.map(task => (
            <tr key={task.name}>
              <td>
                {task.name}
                {task.cancelled > 0 && <small> ({task.cancelled} cancelled)</small>}
              </td>
              <td>{task.completed}</td>
              <td>{ms(task.runP50Us)}</td>
              <td>{ms(task.runP95Us)}</td>
              <td>{ms(task.waitP95Us)}</td>
            </tr>
          ))}
        </tbody>
        {running.length > 0 && (
          <>
            <thead>
              <tr>
                <th>Extensions</th>
                <th>calls</th>
                <th>busy ms</th>
                <th>slowest</th>
                <th>memory</th>
              </tr>
            </thead>
            <tbody>
              {running.map(extension => {
                const used = usage.get(extension.id)!
                return (
                  <tr key={extension.id}>
                    <td>
                      {extension.name}
                      {used.droppedEvents > 0 && <small> ({used.droppedEvents} dropped)</small>}
                    </td>
                    <td>{used.calls}</td>
                    <td>{used.busyMs.toFixed(used.busyMs < 10 ? 2 : 0)}</td>
                    <td>{used.slowestCallMs.toFixed(used.slowestCallMs < 10 ? 2 : 0)}</td>
                    <td>{used.memoryBytes > 0 ? mb(used.memoryBytes) : '—'}</td>
                  </tr>
                )
              })}
            </tbody>
          </>
        )}
      </table>
      <p className="perf-overlay__memory">
        {backend?.backendMemoryBytes != null && <>Backend {mb(backend.backendMemoryBytes)} · </>}
        {heap && <>JS heap {mb(heap.usedJSHeapSize)} · </>}
        {backend && (
          <>
            {backend.openDocuments} documents ({mb(backend.openDocumentBytes)}) ·{' '}
            {backend.queuedTasks} queued
          </>
        )}
      </p>
    </aside>
  )
}

function format(value: number, unit: SeriesSummary['unit']): string {
  return unit === 'ms' ? value.toFixed(value < 10 ? 2 : 0) : String(Math.round(value))
}
