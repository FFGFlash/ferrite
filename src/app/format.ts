import type { AppInfo } from "../ipc";

const osNames: Record<string, string> = {
  windows: "Windows",
  macos: "macOS",
  linux: "Linux",
};

export function formatPlatform(info: Pick<AppInfo, "os" | "arch">): string {
  return `${osNames[info.os] ?? info.os} ${info.arch}`;
}
