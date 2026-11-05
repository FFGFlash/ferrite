/** Typed wrappers for the log commands (`src-tauri/src/logging.rs`). */
import { call } from './index'

export type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace'

/** A log event, as the Output view shows it. */
export interface LogRecord {
  /** Increases with each record, across channels. */
  seq: number
  /** Milliseconds since the Unix epoch. */
  time: number
  level: LogLevel
  target: string
  message: string
  /** The event's other fields, as `name=value` pairs. */
  fields: string
}

export interface LogChannel {
  /** `flareite`, `extension:<id>`, or `server:<name>`. */
  id: string
  /** The newest record's `seq`. */
  last: number
}

export interface Logs {
  channels: LogChannel[]
  /** The channel's records after the `seq` asked for. */
  records: LogRecord[]
}

/** The channels with records, and `channel`'s records after `after` (0 for all kept). */
export const readLogs = (channel: string, after: number) =>
  call<Logs>('logs_read', { channel, after })
/** Today's log file, which has everything since the app started; `null` without one. */
export const logFilePath = () => call<string | null>('logs_file')
