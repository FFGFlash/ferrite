import type { AppInfo } from '../ipc'
import type { ResourceUsage } from '../ipc/extensions'

const osNames: Record<string, string> = {
  windows: 'Windows',
  macos: 'macOS',
  linux: 'Linux',
}

const megabytes = (bytes: number) => `${(bytes / (1024 * 1024)).toFixed(1)} MB`
const milliseconds = (ms: number) => `${ms.toFixed(ms < 10 ? 1 : 0)} ms`

/**
 * An extension's resource use for the Extensions view, e.g. "Ran 12 times
 * for 3.4 ms in total (slowest 1.2 ms) · 1.1 MB memory", or `null` before
 * it has run.
 */
export function formatUsage(usage: ResourceUsage): string | null {
  if (usage.calls === 0) {
    return null
  }
  const times = usage.calls === 1 ? 'once' : `${usage.calls} times`
  const parts = [
    `Ran ${times} for ${milliseconds(usage.busyMs)} in total (slowest ${milliseconds(usage.slowestCallMs)})`,
  ]
  if (usage.memoryBytes > 0) {
    parts.push(`${megabytes(usage.memoryBytes)} memory`)
  }
  if (usage.droppedEvents > 0) {
    parts.push(
      `${usage.droppedEvents} ${usage.droppedEvents === 1 ? 'change' : 'changes'} skipped while busy`,
    )
  }
  return parts.join(' · ')
}

export function formatPlatform(info: Pick<AppInfo, 'os' | 'arch'>): string {
  return `${osNames[info.os] ?? info.os} ${info.arch}`
}
