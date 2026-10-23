/**
 * The command registry: every workbench action is a named command with an
 * optional keybinding. The command palette lists them, the keybinding
 * dispatcher runs them, and extension contributions (M7) will register into
 * the same registry.
 */
import { useEffect } from "react";
import type { KeyInput } from "../editor/keymap";
import { isMac } from "../editor/keymap";
import { reportError } from "../state/notifications";
import { createStore, useStore } from "../state/store";

/**
 * A key combination such as `"mod+shift+p"` or `"f3"`. `mod` is Ctrl on
 * Windows and Linux and Cmd on macOS; `ctrl`, `alt`, `shift`, and `meta`
 * name keys literally.
 */
export type Keybinding = string | { default: string; mac: string };

export interface Command {
  id: string;
  title: string;
  category?: string;
  keybinding?: Keybinding | Keybinding[];
  /** Whether the command can run now. Defaults to always. */
  enabled?: () => boolean;
  /** Only run from the keyboard while the editor has focus. */
  editorFocus?: boolean;
  /** Omitted from the command palette. */
  hidden?: boolean;
  run: () => void | Promise<void>;
}

const registry = createStore<ReadonlyMap<string, Command>>(new Map());

/** Registers commands; the returned function unregisters them. */
export function registerCommands(commands: readonly Command[]): () => void {
  registry.update((current) => {
    const next = new Map(current);
    for (const command of commands) {
      next.set(command.id, command);
    }
    return next;
  });
  return () =>
    registry.update((current) => {
      const next = new Map(current);
      for (const command of commands) {
        if (next.get(command.id) === command) {
          next.delete(command.id);
        }
      }
      return next;
    });
}

export function getCommand(id: string): Command | undefined {
  return registry.get().get(id);
}

export function useCommands(): Command[] {
  return [...useStore(registry).values()];
}

export function commandLabel(command: Command): string {
  return command.category ? `${command.category}: ${command.title}` : command.title;
}

export async function executeCommand(id: string): Promise<void> {
  const command = getCommand(id);
  if (!command || (command.enabled && !command.enabled())) {
    return;
  }
  try {
    await command.run();
  } catch (error) {
    reportError(commandLabel(command), error);
  }
}

function bindingsOf(command: Command, mac: boolean): string[] {
  const list = command.keybinding === undefined ? [] : [command.keybinding].flat();
  return list.map((binding) =>
    typeof binding === "string" ? binding : mac ? binding.mac : binding.default,
  );
}

interface ParsedBinding {
  key: string;
  ctrl: boolean;
  meta: boolean;
  alt: boolean;
  shift: boolean;
}

function parse(binding: string, mac: boolean): ParsedBinding {
  // Split on "+" but keep a literal "+" key ("mod++").
  const parts = binding.toLowerCase().split(/\+(?!$)/);
  const key = parts.pop() ?? "";
  const has = (name: string) => parts.includes(name);
  return {
    key,
    ctrl: has("ctrl") || (!mac && has("mod")),
    meta: has("meta") || (mac && has("mod")),
    alt: has("alt"),
    shift: has("shift"),
  };
}

export function matchesKeybinding(binding: string, event: KeyInput, mac: boolean): boolean {
  const parsed = parse(binding, mac);
  return (
    parsed.key === event.key.toLowerCase() &&
    parsed.ctrl === event.ctrlKey &&
    parsed.meta === event.metaKey &&
    parsed.alt === event.altKey &&
    parsed.shift === event.shiftKey
  );
}

const keyNames: Record<string, string> = {
  pageup: "PageUp",
  pagedown: "PageDown",
  tab: "Tab",
  enter: "Enter",
  escape: "Esc",
};

/** A keybinding for display, e.g. "Ctrl+Shift+P" or "⇧⌘P". */
export function formatKeybinding(binding: string, mac: boolean): string {
  const parsed = parse(binding, mac);
  const key = keyNames[parsed.key] ?? parsed.key.toUpperCase();
  if (mac) {
    return `${parsed.ctrl ? "⌃" : ""}${parsed.alt ? "⌥" : ""}${parsed.shift ? "⇧" : ""}${parsed.meta ? "⌘" : ""}${key}`;
  }
  return [
    parsed.ctrl && "Ctrl",
    parsed.meta && "Win",
    parsed.alt && "Alt",
    parsed.shift && "Shift",
    key,
  ]
    .filter(Boolean)
    .join("+");
}

/** The first keybinding of `command` on this platform, for display. */
export function primaryKeybinding(command: Command | undefined, mac = isMac()): string | null {
  const first = command ? bindingsOf(command, mac)[0] : undefined;
  return first ? formatKeybinding(first, mac) : null;
}

/** The enabled command bound to `event`, if any. */
export function commandForKey(
  event: KeyInput,
  mac: boolean,
  editorFocused: boolean,
): Command | undefined {
  for (const command of registry.get().values()) {
    if (command.editorFocus && !editorFocused) {
      continue;
    }
    if (!bindingsOf(command, mac).some((binding) => matchesKeybinding(binding, event, mac))) {
      continue;
    }
    if (!command.enabled || command.enabled()) {
      return command;
    }
  }
  return undefined;
}

/** Runs bound commands for key presses anywhere in the window. */
export function useKeybindings(): void {
  useEffect(() => {
    const mac = isMac();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.isComposing) {
        return;
      }
      const target = event.target as HTMLElement | null;
      const editorFocused = target?.dataset.editorInput !== undefined;
      const command = commandForKey(event, mac, editorFocused);
      if (command) {
        event.preventDefault();
        event.stopPropagation();
        void executeCommand(command.id);
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);
}
