/** The workbench's built-in commands and their default keybindings. */
import { openPalette } from "../components/paletteState";
import { activeEditor } from "../editor/activeEditor";
import {
  activateRelative,
  close,
  documentsStore,
  newFile,
  openFile,
  openPath,
  save,
} from "../state/documents";
import { setTheme, settingsFilePath } from "../ipc/workspace";
import { layoutStore, updateLayout } from "../state/layout";
import { closeFolder, collapseAll, openFolder, workspaceStore } from "../state/workspace";
import type { Command } from "./registry";

const hasEditor = () => activeEditor.get() !== null;
const hasDocument = () => documentsStore.get().activeId !== null;
const hasWorkspace = () => workspaceStore.get().info !== null;
const editor = () => activeEditor.get();

function showView(id: string): void {
  updateLayout({ activeView: id, sidebarVisible: true });
  if (id === "search") {
    // Let the panel mount, then focus its query field.
    requestAnimationFrame(() => document.getElementById("search-panel-query")?.focus());
  }
}

export const builtinCommands: Command[] = [
  { id: "file.newFile", category: "File", title: "New File", keybinding: "mod+n", run: newFile },
  {
    id: "file.openFile",
    category: "File",
    title: "Open File…",
    keybinding: "mod+o",
    run: openFile,
  },
  { id: "file.openFolder", category: "File", title: "Open Folder…", run: openFolder },
  {
    id: "file.closeFolder",
    category: "File",
    title: "Close Folder",
    enabled: hasWorkspace,
    run: closeFolder,
  },
  {
    id: "file.save",
    category: "File",
    title: "Save",
    keybinding: "mod+s",
    enabled: hasDocument,
    run: () => save(),
  },
  {
    id: "file.saveAs",
    category: "File",
    title: "Save As…",
    keybinding: "mod+shift+s",
    enabled: hasDocument,
    run: () => save(undefined, true),
  },
  {
    id: "file.close",
    category: "View",
    title: "Close Editor",
    keybinding: "mod+w",
    enabled: hasDocument,
    run: () => close(),
  },
  {
    id: "workbench.commandPalette",
    category: "View",
    title: "Command Palette",
    keybinding: ["mod+shift+p", "f1"],
    hidden: true,
    run: () => openPalette(">"),
  },
  {
    id: "workbench.quickOpen",
    category: "Go",
    title: "Go to File…",
    keybinding: "mod+p",
    run: () => openPalette(""),
  },
  {
    id: "workbench.goToLine",
    category: "Go",
    title: "Go to Line…",
    keybinding: "ctrl+g",
    enabled: hasEditor,
    run: () => openPalette(":"),
  },
  {
    id: "workbench.toggleSidebar",
    category: "View",
    title: "Toggle Side Bar",
    keybinding: "mod+b",
    run: () => updateLayout({ sidebarVisible: !layoutStore.get().sidebarVisible }),
  },
  {
    id: "workbench.showExplorer",
    category: "View",
    title: "Show Explorer",
    keybinding: "mod+shift+e",
    run: () => showView("explorer"),
  },
  {
    id: "workbench.showSearch",
    category: "View",
    title: "Search in Files",
    keybinding: "mod+shift+f",
    run: () => showView("search"),
  },
  {
    id: "explorer.collapseAll",
    category: "Explorer",
    title: "Collapse Folders",
    enabled: hasWorkspace,
    run: collapseAll,
  },
  {
    id: "view.nextEditor",
    category: "View",
    title: "Next Editor",
    keybinding: ["ctrl+tab", "ctrl+pagedown"],
    enabled: hasDocument,
    run: () => activateRelative(1),
  },
  {
    id: "view.previousEditor",
    category: "View",
    title: "Previous Editor",
    keybinding: ["ctrl+shift+tab", "ctrl+pageup"],
    enabled: hasDocument,
    run: () => activateRelative(-1),
  },
  {
    id: "editor.find",
    category: "Edit",
    title: "Find",
    keybinding: "mod+f",
    enabled: hasEditor,
    run: () => editor()?.openFind(false),
  },
  {
    id: "editor.replace",
    category: "Edit",
    title: "Replace",
    keybinding: { default: "mod+h", mac: "mod+alt+f" },
    enabled: hasEditor,
    run: () => editor()?.openFind(true),
  },
  {
    id: "editor.findNext",
    category: "Edit",
    title: "Find Next",
    keybinding: "f3",
    enabled: hasEditor,
    run: () => editor()?.findNext(false),
  },
  {
    id: "editor.findPrevious",
    category: "Edit",
    title: "Find Previous",
    keybinding: "shift+f3",
    enabled: hasEditor,
    run: () => editor()?.findNext(true),
  },
  {
    id: "editor.addNextMatch",
    category: "Selection",
    title: "Add Next Occurrence",
    keybinding: "mod+d",
    editorFocus: true,
    enabled: hasEditor,
    run: () => editor()?.dispatch({ type: "addNextMatch" }),
  },
  {
    id: "editor.selectAll",
    category: "Selection",
    title: "Select All",
    enabled: hasEditor,
    run: () => editor()?.dispatch({ type: "selectAll" }),
  },
  {
    id: "editor.undo",
    category: "Edit",
    title: "Undo",
    enabled: hasEditor,
    run: () => editor()?.dispatch({ type: "undo" }),
  },
  {
    id: "editor.redo",
    category: "Edit",
    title: "Redo",
    enabled: hasEditor,
    run: () => editor()?.dispatch({ type: "redo" }),
  },
  {
    id: "preferences.openSettings",
    category: "Preferences",
    title: "Open Settings (TOML)",
    keybinding: "mod+,",
    run: async () => openPath(await settingsFilePath()),
  },
  {
    id: "preferences.themeDark",
    category: "Preferences",
    title: "Color Theme: Dark",
    run: () => setTheme("dark"),
  },
  {
    id: "preferences.themeLight",
    category: "Preferences",
    title: "Color Theme: Light",
    run: () => setTheme("light"),
  },
  {
    id: "preferences.themeSystem",
    category: "Preferences",
    title: "Color Theme: Follow System",
    run: () => setTheme("system"),
  },
];
