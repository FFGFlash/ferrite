import { executeCommand } from "../commands/registry";
import {
  restartExtension,
  setExtensionEnabled,
  setExtensionPermissions,
  type ExtensionState,
  type ExtensionStatus,
  type Permission,
} from "../ipc/extensions";
import { extensionsStore } from "../state/extensions";
import { reportError } from "../state/notifications";
import { useStore } from "../state/store";
import "./ExtensionsPanel.css";

const stateLabels: Record<ExtensionState, string> = {
  inactive: "Inactive",
  awaitingPermission: "Needs permission",
  starting: "Starting",
  active: "Active",
  failed: "Stopped",
  disabled: "Disabled",
  invalid: "Invalid",
};

/** Lists installed and development extensions with their state and permissions. */
export function ExtensionsPanel() {
  const extensions = useStore(extensionsStore);
  if (extensions.length === 0) {
    return (
      <div className="extensions-panel__empty">
        <p>No extensions are installed.</p>
        <p>
          Put extension folders in the <code>extensions</code> folder next to your settings file, or
          load one you are developing.
        </p>
        <button
          type="button"
          className="extensions-panel__button"
          onClick={() => void executeCommand("extensions.addDevelopment")}
        >
          Load Development Extension…
        </button>
      </div>
    );
  }
  return (
    <ul className="extensions-panel" aria-label="Extensions">
      {extensions.map((extension) => (
        <ExtensionItem key={extension.id} extension={extension} />
      ))}
    </ul>
  );
}

function act(action: string, run: () => Promise<void>): void {
  run().catch((error: unknown) => reportError(action, error));
}

function ExtensionItem({ extension }: { extension: ExtensionStatus }) {
  const valid = extension.state !== "invalid";
  const disabled = extension.state === "disabled";
  const granted = extension.permissions.filter((p) => p.granted).map((p) => p.permission);
  const toggle = (permission: Permission, allow: boolean) => {
    const next = allow ? [...granted, permission] : granted.filter((p) => p !== permission);
    act("Changing permissions", () => setExtensionPermissions(extension.id, next));
  };

  return (
    <li className="extension" data-state={extension.state}>
      <div className="extension__header">
        <span className="extension__name">{extension.name}</span>
        {extension.version && <span className="extension__version">{extension.version}</span>}
        {extension.development && <span className="extension__badge">dev</span>}
        <span className="extension__state">{stateLabels[extension.state]}</span>
      </div>
      {extension.description && <p className="extension__description">{extension.description}</p>}
      {extension.error && (
        <p className="extension__error" role="alert">
          {extension.error}
        </p>
      )}
      {extension.configErrors.map((error) => (
        <p key={error} className="extension__warning">
          Setting: {error}
        </p>
      ))}
      {extension.permissions.length > 0 && (
        <fieldset className="extension__permissions">
          <legend>Permissions</legend>
          {extension.permissions.map((permission) => (
            <label key={permission.permission} className="extension__permission">
              <input
                type="checkbox"
                checked={permission.granted}
                onChange={(event) => toggle(permission.permission, event.currentTarget.checked)}
              />
              <span>
                {permission.description}
                {!permission.decided && <em> (not decided)</em>}
              </span>
            </label>
          ))}
        </fieldset>
      )}
      {extension.commands.length > 0 && (
        <div className="extension__commands">
          {extension.commands.map((command) => (
            <button
              key={command.id}
              type="button"
              className="extension__command"
              disabled={disabled}
              title={`Run ${command.category}: ${command.title}`}
              onClick={() => void executeCommand(command.id)}
            >
              {command.title}
            </button>
          ))}
        </div>
      )}
      <div className="extension__footer">
        <span className="extension__path" title={extension.path}>
          {extension.path}
        </span>
        {valid && (
          <>
            <button
              type="button"
              className="extensions-panel__button"
              disabled={disabled}
              onClick={() => act("Restarting the extension", () => restartExtension(extension.id))}
            >
              Restart
            </button>
            <button
              type="button"
              className="extensions-panel__button"
              onClick={() =>
                act(disabled ? "Enabling the extension" : "Disabling the extension", () =>
                  setExtensionEnabled(extension.id, disabled),
                )
              }
            >
              {disabled ? "Enable" : "Disable"}
            </button>
          </>
        )}
      </div>
    </li>
  );
}

/** Buttons in the side bar header. */
export function ExtensionsActions() {
  return (
    <>
      <button
        type="button"
        className="sidebar__action"
        title="Load Development Extension…"
        aria-label="Load Development Extension"
        onClick={() => void executeCommand("extensions.addDevelopment")}
      >
        ⊕
      </button>
      <button
        type="button"
        className="sidebar__action"
        title="Reload Extensions"
        aria-label="Reload Extensions"
        onClick={() => void executeCommand("extensions.reload")}
      >
        ↻
      </button>
    </>
  );
}
