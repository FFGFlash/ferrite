/** Typed wrappers for the terminal commands (`src-tauri/src/terminal.rs`). */
import { Channel } from '@tauri-apps/api/core'
import { call } from './index'

export interface TerminalInfo {
  id: number
  /** The profile's name, such as `PowerShell` or `Ubuntu (WSL)`. */
  title: string
}

/** A shell to start terminals with: found on this computer, or configured. */
export interface TerminalProfile {
  name: string
  /** Whether new terminals use it unless another is chosen. */
  default: boolean
}

/** The installed shells and configured profiles, the default marked. */
export function terminalProfiles(): Promise<TerminalProfile[]> {
  return call<TerminalProfile[]>('terminal_profiles')
}

/** Makes `profile` the default, in the settings file. */
export function setDefaultProfile(profile: string): Promise<void> {
  return call<void>('settings_set_default_profile', { profile })
}

export type TerminalMessage =
  | { type: 'output'; data: string }
  /** Output from before this channel was attached (after a window reload). */
  | { type: 'replay'; data: string }
  /** Always the last message. `code` is null if it couldn't be read. */
  | { type: 'exited'; code: number | null }

function channel(onMessage: (message: TerminalMessage) => void): Channel<TerminalMessage> {
  const channel = new Channel<TerminalMessage>()
  channel.onmessage = onMessage
  return channel
}

export interface TerminalPlatform {
  /** The Windows build: ConPTY's behavior on resize depends on it. */
  windowsBuild: number | null
}

export function terminalPlatform(): Promise<TerminalPlatform> {
  return call<TerminalPlatform>('terminal_platform')
}

/**
 * Starts a shell with a profile (the default if `null`); its output arrives
 * through `onMessage`.
 */
export function createTerminal(
  cols: number,
  rows: number,
  profile: string | null,
  onMessage: (message: TerminalMessage) => void,
): Promise<TerminalInfo> {
  return call<TerminalInfo>('terminal_create', {
    cols,
    rows,
    profile,
    channel: channel(onMessage),
  })
}

/** The terminals still running in the backend (after a window reload). */
export function listTerminals(): Promise<TerminalInfo[]> {
  return call<TerminalInfo[]>('terminal_list')
}

/** Sends a terminal's output to `onMessage`, starting with what it printed so far. */
export function attachTerminal(
  id: number,
  onMessage: (message: TerminalMessage) => void,
): Promise<void> {
  return call<void>('terminal_attach', { id, channel: channel(onMessage) })
}

export function writeTerminal(id: number, data: string): Promise<void> {
  return call<void>('terminal_write', { id, data })
}

export function resizeTerminal(id: number, cols: number, rows: number): Promise<void> {
  return call<void>('terminal_resize', { id, cols, rows })
}

/** Reports that `units` (UTF-16 code units) of output were drawn, for flow control. */
export function acknowledgeTerminal(id: number, units: number): Promise<void> {
  return call<void>('terminal_acknowledge', { id, units })
}

/** Ends the shell and forgets the terminal. */
export function killTerminal(id: number): Promise<void> {
  return call<void>('terminal_kill', { id })
}
