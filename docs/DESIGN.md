# Project: A High-Performance, Extensible Code Editor

## 1. Project Overview

Build a fast, memory-efficient, cross-platform code editor using Rust and Tauri, with a React-based frontend and a WebAssembly (WASM) extension system.

The editor should offer functionality comparable to VS Code and Atom, but with an architecture designed around performance, memory efficiency, security, and extensibility.

The editor should support a customizable workbench where extensions can modify the editor's appearance and behavior, contribute UI components, register commands, add language support, and introduce new functionality.

The core application must remain independent of individual extensions. Extensions should interact with the editor through a stable, versioned API rather than accessing internal implementation details.

The project should be developed incrementally, with a working application at every major stage.

### Primary goals

1. Fast application startup.
2. Low baseline memory consumption.
3. Responsive editing, including large files.
4. Incremental parsing and background processing.
5. A flexible, customizable UI.
6. A secure WASM extension runtime.
7. An extensible API inspired by VS Code's extension model.
8. Cross-platform support for Windows, Linux, and macOS.
9. A maintainable Rust workspace with clearly defined architectural boundaries.
10. Automated tests and repeatable performance benchmarks.

### Non-goals for the initial release

Do not initially attempt to:

- Reproduce the entire VS Code API.
- Run existing VS Code extensions unchanged.
- Build an extension marketplace.
- Implement every feature of a full IDE.
- Implement a custom programming language or compiler.
- Develop custom language servers for every language.
- Optimize every subsystem before measuring actual performance.

These capabilities may be considered later, but they must not compromise the initial architecture.

---

# 2. Technology Stack

Use the following technologies unless a technical limitation justifies a documented alternative.

## Backend

- Rust, using a stable toolchain.
- Tauri for desktop application integration.
- Tokio for asynchronous I/O and background tasks where appropriate.
- Ropey for efficient text buffer storage.
- Tree-sitter for incremental syntax parsing.
- A Rust LSP client implementation for language-server communication.
- Wasmtime for executing WASM extensions.
- WIT and the WebAssembly Component Model for defining extension interfaces where practical.
- `tracing` for structured logging and diagnostics.
- TOML for human-readable application and extension configuration where appropriate.
- A cross-platform filesystem notification library for workspace change detection.

Prefer established, maintained libraries over custom implementations when they meet the project's requirements.

Before introducing a dependency, verify its current maintenance status, compatibility with the selected toolchain, licensing, and suitability for the intended use.

Do not invent APIs or assume that a particular library supports a feature without checking its documentation.

## Frontend

- React.
- TypeScript.
- Vite, using the integration supported by the selected Tauri version.
- CSS variables and semantic design tokens for theming.
- A component architecture that supports extension-provided UI.
- A frontend state-management strategy selected for simplicity and predictable updates.

Keep the frontend independent of Rust implementation details. Use Tauri commands and events as the application boundary.

Avoid introducing a large state-management framework unless the project's actual requirements justify it.

## Extension development

- WASM modules executed by Wasmtime.
- WIT-defined interfaces where supported by the chosen Component Model implementation.
- A documented extension manifest.
- A versioned extension API.
- Rust as the initial first-class extension development language.

The architecture should allow other languages that can compile to a compatible WASM interface to be supported later.

Do not require every extension to be written in Rust.

---

# 3. Architectural Principles

These principles are mandatory.

## 3.1 Separate the application into distinct layers

The project must separate:

1. Desktop application lifecycle.
2. Frontend and workbench UI.
3. Editor core.
4. Text storage and editing.
5. Language services.
6. Workspace and filesystem management.
7. Extension API.
8. Extension runtime and security.
9. Persistent settings and application state.

Each layer should have a clear responsibility and a documented interface.

Avoid circular dependencies.

The editor core must not depend on React, Tauri frontend implementation details, or a specific extension.

The extension runtime must not require unrestricted access to editor internals.

## 3.2 The Rust core owns authoritative document state

Rust must own the authoritative editable document model.

The React frontend should render document state and submit editing operations through a well-defined interface.

Do not maintain an independent, complete copy of every document in the frontend merely to simplify state management.

However, do not send a Rust-to-React IPC message for every internal operation if doing so would introduce unnecessary overhead. Measure the costs and choose an efficient communication strategy.

The architecture should support batched updates, incremental document changes, and viewport-oriented rendering.

## 3.3 Responsiveness is a first-class requirement

Typing and basic cursor movement must not wait for:

- Workspace indexing.
- Extension initialization.
- Language-server startup.
- Expensive syntax analysis.
- Filesystem scans.
- Network requests.
- Extension callbacks.

Perform expensive work asynchronously or on appropriate background workers.

Avoid synchronous frontend calls for long-running operations.

## 3.4 Prefer incremental computation

Where practical, update only the affected portions of:

- Text indexes.
- Syntax trees.
- Diagnostics.
- Workspace indexes.
- Search results.
- Rendered document state.

Avoid full-document processing after every keystroke unless necessary.

## 3.5 Optimize based on measurements

Do not make unsupported claims that the editor will outperform VS Code or use a specific amount of memory.

Establish benchmarks and measure actual behavior.

Record the reasoning behind major performance-related architectural decisions.

---

# 4. Application Architecture

Organize the application as a Cargo workspace.

Use the following as an initial logical structure. Adjust crate boundaries when implementation experience demonstrates a concrete benefit, but document significant changes.

```text
editor/
├── Cargo.toml
├── Cargo.lock
├── package.json
├── crates/
│   ├── editor-core/
│   ├── editor-text/
│   ├── editor-syntax/
│   ├── editor-lsp/
│   ├── editor-workspace/
│   ├── editor-extensions-api/
│   ├── editor-extension-host/
│   └── editor-app/
├── src-tauri/
├── src/
│   ├── app/
│   ├── components/
│   ├── editor/
│   ├── panels/
│   ├── extensions/
│   ├── themes/
│   ├── state/
│   └── styles/
├── extensions/
│   └── examples/
├── tests/
│   ├── integration/
│   └── fixtures/
├── benches/
└── docs/
    ├── architecture.md
    ├── extension-api.md
    ├── security.md
    └── performance.md
```

Do not create empty crates merely to match this tree.

Create components when they have real responsibilities and useful interfaces.

## 4.1 `editor-text`

Responsible for:

- Text buffer storage.
- Efficient text insertion and deletion.
- Text-position conversion.
- Line indexing.
- Document snapshots where necessary.
- Document versioning.
- Efficient change descriptions.

Use Ropey initially.

Define a consistent internal position representation. Prefer UTF-8 byte offsets or another representation that can be converted efficiently, but explicitly document the choice.

Handle Unicode correctly, including multi-byte characters, combining marks, and line endings.

Implement conversion functions for LSP positions and frontend coordinates at the relevant boundaries.

Avoid ambiguous APIs that mix byte offsets, character counts, and line/column positions.

## 4.2 `editor-core`

Responsible for:

- Open documents.
- Document lifecycle.
- Cursor positions and selections.
- Multiple cursors.
- Editing operations.
- Undo and redo.
- Dirty state.
- Saving coordination.
- Document events.
- Editor-level commands.
- View state where appropriate.

The editor core must remain independent of the UI framework.

Undo history should be efficient and have a configurable retention policy.

## 4.3 `editor-syntax`

Responsible for:

- Tree-sitter integration.
- Incremental parsing.
- Syntax highlighting data.
- Language-to-grammar configuration.
- Syntax-related caches.

Do not require language servers for basic syntax highlighting.

Parsing must not block interactive editing.

## 4.4 `editor-workspace`

Responsible for:

- Workspace roots.
- File discovery.
- File opening and saving.
- Filesystem notifications.
- Ignore rules.
- File search.
- Incremental indexing.
- External file modifications.

Do not eagerly read every workspace file into memory.

Support excluding irrelevant directories, generated files, and large files according to configurable rules.

## 4.5 `editor-lsp`

Responsible for:

- Language-server process management.
- LSP client communication.
- Document synchronization.
- Diagnostics.
- Completion.
- Go to definition.
- References.
- Rename.
- Hover information.
- Code actions.

Start language servers on demand where practical.

Reuse suitable server processes across documents and workspaces when supported.

Implement cancellation and stale-response handling.

## 4.6 `editor-extensions-api`

Defines the public extension interface.

It must not expose private editor-core types directly.

Keep the public API versioned and documented.

The extension API should be usable independently of the editor's internal implementation.

## 4.7 `editor-extension-host`

Responsible for:

- Extension discovery.
- Manifest parsing and validation.
- WASM module loading.
- Extension lifecycle.
- Permission enforcement.
- Execution limits.
- Host-to-extension communication.
- Extension event delivery.
- Error reporting.
- Extension shutdown and cancellation.

Do not allow extensions to execute unrestricted native code through the normal extension API.

## 4.8 `editor-app` and `src-tauri`

Responsible for application initialization, dependency wiring, Tauri commands, frontend integration, window lifecycle, and operating-system integration.

Keep application-specific initialization out of the editor core.

---

# 5. UI and Workbench

The UI should use Tauri and React, with the flexibility to customize the workbench through extensions.

The interface should be inspired by VS Code and Atom without copying their branding or relying on their implementation.

## 5.1 Initial layout

Implement a functional workbench containing:

- Application menu or equivalent command access.
- Activity bar.
- File explorer.
- Editor area.
- Editor tabs.
- Status bar.
- Command palette.
- Search panel.
- Settings interface.
- Basic notifications and dialogs.

Use a clean, configurable layout.

Support resizing panels and remembering layout preferences.

Do not attempt to build every panel immediately. Prioritize a usable editor, file explorer, tabs, and command palette.

## 5.2 Editor rendering

The editor must support:

- Line numbers.
- Syntax highlighting.
- Cursor and selection rendering.
- Scrolling.
- Find and replace.
- Multiple selections.
- Keyboard navigation.
- Basic text editing.
- Large files.
- Proper Unicode handling.

The rendering architecture should avoid constructing expensive UI structures for every line in a large document.

Use viewport-oriented rendering and efficient update strategies.

Do not implement a custom text-shaping engine unless the selected UI architecture makes it necessary.

Account for IME input, clipboard operations, accessibility, and platform-specific keyboard behavior.

## 5.3 Customizable appearance

The editor's appearance must be extensible.

Use semantic CSS variables and design tokens for:

- Editor background and foreground.
- Selection and cursor colors.
- Syntax colors.
- Panel backgrounds.
- Borders.
- Focus indicators.
- Typography.
- Spacing.
- Status-bar colors.
- Theme-specific states.

Support dark and light themes.

Allow extensions to contribute themes and theme-related configuration.

Avoid scattering hard-coded colors throughout React components.

## 5.4 Extension-provided UI

Design a safe extension UI contribution model.

Extensions should eventually be able to contribute:

- Commands.
- Menus.
- Keybindings.
- Status-bar items.
- Activity-bar entries.
- Sidebar panels.
- Editor decorations.
- Themes.
- Configuration controls.
- Context-menu actions.
- Custom views.

Because extensions execute in WASM rather than directly in the browser's JavaScript environment, do not assume an extension can simply export a React component.

Instead, define a host-managed contribution system.

For the initial implementation, prefer declarative UI contributions: extensions register a contribution descriptor and the host renders the corresponding UI.

For richer custom views, investigate a structured UI protocol in which extensions emit validated UI descriptions and receive user-interaction events.

Do not expose unrestricted DOM access or arbitrary JavaScript execution as part of the WASM API.

A future extension may optionally use a separately sandboxed webview if a compelling use case justifies the additional security and memory costs.

The architecture must not require a separate webview for every extension.

---

# 6. WASM Extension System

This is a core product feature, not an optional future architecture.

However, implement it incrementally.

## 6.1 Extension manifest

Create a documented manifest format.

An illustrative manifest:

```toml
id = "example.hello-world"
name = "Hello World"
version = "0.1.0"
api_version = "1"
entrypoint = "extension.wasm"
description = "An example editor extension."

[permissions]
workspace_read = true
workspace_write = false
network = false
```

This is an initial design sketch, not a requirement to preserve these exact fields.

The final schema should define:

- Unique extension identifier.
- Name and description.
- Version.
- Entry point.
- Supported editor and API versions.
- Activation conditions.
- Dependencies where appropriate.
- Declared permissions.
- Contributions.
- Optional configuration schema.

Validate manifests before loading extensions.

Reject unsupported API versions and malformed declarations with clear errors.

## 6.2 WASM runtime

Use Wasmtime.

Investigate the WebAssembly Component Model and WIT support provided by the selected Wasmtime version.

Prefer typed interfaces and generated bindings over an ad hoc binary protocol.

Pin compatible dependency versions and document any limitations imposed by the selected WASM component model.

If a required feature is unavailable or impractical, document the limitation and choose the simplest secure alternative.

Do not silently replace the planned extension architecture with unrestricted native plugins.

## 6.3 Initial extension API

Implement the extension API in stages.

The first usable API should support:

1. Extension initialization and shutdown.
2. Extension metadata.
3. Command registration.
4. Command execution.
5. Document identification and read access.
6. Document-change notifications.
7. Applying validated text edits.
8. Publishing diagnostics.
9. Reading extension configuration.
10. Logging through the host.

Later additions may include:

- Theme registration.
- Keybindings.
- Menu contributions.
- Status-bar contributions.
- Sidebar panels.
- Editor decorations.
- Language registrations.
- Workspace search.
- LSP integration.
- Rich custom views.

Keep the API small until its semantics are well tested.

## 6.4 Document API

Do not provide unrestricted mutable access to editor internals.

Expose document information through explicit operations.

Support:

- Stable document identifiers.
- Document versions.
- Read-only access to text or requested ranges.
- Change notifications.
- Validated text edits.
- Explicit error handling.

Avoid repeatedly transferring the entire document across the WASM boundary.

For large documents, provide range-based access and incremental change information.

If snapshots are needed, define their ownership and lifetime explicitly.

## 6.5 Text edits

Extensions must not be able to mutate the core's document storage directly.

Instead, extensions submit edit operations.

The host must validate:

- Document identity.
- Document version.
- Edit ranges.
- Permissions.
- Operation limits.

Define how conflicting edits are handled.

Use transactions where multiple edits must succeed or fail together.

Reject stale or invalid edits with structured errors.

## 6.6 Permissions and isolation

Extensions should run with least privilege.

By default, extensions must not have unrestricted access to:

- The filesystem.
- Network access.
- Process creation.
- Environment variables.
- Arbitrary operating-system APIs.
- Other extensions' private state.

Expose these capabilities through host-controlled interfaces.

Only grant capabilities that the user or administrator has authorized.

A manifest declaration is not permission by itself.

The runtime and host must enforce the effective permissions.

Implement resource controls appropriate to the chosen runtime, including:

- WASM memory limits.
- CPU or fuel limits where supported.
- Limits on extension output.
- Cancellation of long-running work.
- Limits on queued events and pending operations.
- Clear failure reporting.
- Safe extension shutdown.

Treat malicious and malfunctioning extensions as expected failure cases.

Do not claim that WASM automatically prevents every security problem. Review the actual host capabilities and execution environment.

## 6.7 Extension scheduling

Extension execution must not block typing or normal UI interaction.

Use background execution and asynchronous communication.

Do not invoke arbitrary extension code synchronously from a UI event handler.

Implement:

- Event batching or coalescing where appropriate.
- Cancellation.
- Backpressure.
- Stale-result detection.
- Error isolation.
- Per-extension resource accounting.

An extension that crashes, exceeds its resource budget, or enters an infinite computation should not crash the entire editor.

## 6.8 Extension developer experience

Provide an example extension and a minimal development workflow.

The repository should eventually include:

- Extension manifest.
- Example extension source.
- WASM build instructions.
- Development and testing instructions.
- API documentation.
- A way to load a local development extension.
- Integration tests for host-extension communication.

Make it easy to build and test extensions without rebuilding the entire editor unnecessarily.

---

# 7. Language Support

## 7.1 Tree-sitter

Use Tree-sitter for incremental syntax parsing.

The editor should support language grammars without requiring a language server.

Define how grammars are packaged, registered, versioned, and associated with file extensions.

Do not load every possible grammar unnecessarily.

## 7.2 Language Server Protocol

Implement LSP support for semantic language features.

Initially support:

- Starting and stopping language servers.
- Initialization and capability negotiation.
- Document synchronization.
- Diagnostics.
- Completion.
- Hover.
- Go to definition.
- Find references.
- Rename.
- Code actions.

Add features incrementally.

Handle server crashes, cancellation, malformed messages, and stale results.

Do not assume that every language server has identical capabilities.

## 7.3 Workspace indexing

Implement workspace discovery and indexing as background operations.

Requirements:

- Respect ignore rules.
- Avoid reading all files eagerly.
- Avoid retaining complete file contents when unnecessary.
- Update incrementally after filesystem changes.
- Bound caches.
- Allow indexing to be cancelled.
- Keep the editor usable while indexing.
- Provide clear error handling for inaccessible or changing files.

Do not implement a complicated global symbol index until its requirements are clear.

---

# 8. Performance and Memory Requirements

Performance is a primary architectural constraint.

## 8.1 Startup

The application should open its main window and become interactive without waiting for:

- Extension initialization.
- Workspace indexing.
- Language-server startup.
- Full syntax analysis of every open file.

Load expensive services lazily.

Measure cold startup separately from subsequent launches.

## 8.2 Text buffers

Use efficient text storage and incremental operations.

Avoid copying an entire document for a small edit.

Keep the authoritative document in Rust.

Minimize unnecessary document serialization across Tauri IPC and the WASM boundary.

## 8.3 Frontend

Avoid excessive React re-renders.

Use stable component boundaries and incremental updates.

Do not place complete document contents into broad application state if smaller, scoped state can serve the same purpose.

Measure the cost of Rust-to-frontend communication.

Use batched updates and viewport-oriented data transfer when beneficial.

## 8.4 Background work

Keep expensive computation off the main UI thread.

Use cancellation and bounded queues.

Do not spawn an unbounded number of tasks or worker threads.

Coordinate background work to avoid competing for CPU and memory unnecessarily.

## 8.5 Caches

Every significant cache should have:

- A clear owner.
- A defined lifetime.
- A reason to exist.
- A size limit or eviction strategy when appropriate.
- Instrumentation where useful.

Avoid caching data that is cheap to recompute but expensive to retain.

## 8.6 Large files

Implement a deliberate large-file strategy.

Opening a large file must not require constructing an enormous frontend object tree.

Support efficient scrolling and editing.

Consider disabling or limiting expensive features for exceptionally large files, but make the behavior explicit and predictable.

Do not introduce silent data loss or change document contents merely because a file is large.

## 8.7 Benchmarks

Create benchmarks for:

- Startup time.
- Text insertion and deletion.
- Large-file opening.
- Large-file scrolling.
- Undo and redo.
- Syntax parsing.
- Workspace discovery.
- Search.
- Extension invocation overhead.
- Memory usage with no extensions.
- Memory overhead per extension.
- Frontend update latency.

Document how to run each benchmark.

Use representative test files and workspaces.

Establish a baseline before setting aggressive numerical targets.

---

# 9. Error Handling and Reliability

Use structured errors across crate boundaries.

Do not panic for ordinary user errors, malformed extension data, inaccessible files, or recoverable language-server failures.

Log internal diagnostic information while presenting concise messages to users.

Implement:

- File read and write errors.
- External file-change detection.
- Save conflict handling.
- Extension failure isolation.
- WASM resource-limit errors.
- LSP connection failures.
- Invalid configuration handling.
- Unsupported extension API versions.
- Graceful application shutdown.

Do not silently discard user edits.

Implement safe saving behavior, and document the policy for files modified externally while open.

Add tests for failure cases rather than testing only the happy path.

---

# 10. Testing Strategy

Use unit tests, integration tests, and benchmarks.

## Unit tests

At minimum, cover:

- Text insertion and deletion.
- Unicode position handling.
- Line indexing.
- Undo and redo.
- Dirty-state transitions.
- Document versioning.
- Edit validation.
- Manifest validation.
- Permission checks.
- Extension API version compatibility.
- Stale-result rejection.

## Integration tests

Cover:

- Opening, editing, and saving files.
- External filesystem changes.
- Tree-sitter incremental updates.
- LSP initialization and document synchronization.
- Extension loading and shutdown.
- Command registration and execution.
- Extension diagnostics.
- WASM resource-limit enforcement.
- Extension failures without editor failure.

Use mocks or test servers where practical to keep tests deterministic.

## Frontend tests

Cover:

- Editor interaction.
- Tabs.
- File explorer.
- Command palette.
- Settings.
- Theme changes.
- Extension-provided contributions.
- Error and loading states.

## Security tests

Test that an extension cannot access capabilities that were not granted.

Test resource limits and cancellation.

Verify that malformed extension input cannot corrupt document state or bypass permission checks.

---

# 11. Development Workflow

Work incrementally.

Before implementation:

1. Inspect the repository and existing files.
2. Determine whether the project already has an established architecture.
3. Identify relevant toolchain and dependency constraints.
4. Record assumptions and unresolved decisions.
5. Create a short implementation plan.
6. Implement the smallest useful milestone.

Do not overwrite existing project files blindly.

If the repository is empty, initialize the workspace and create the minimum viable application structure.

For each milestone:

1. Explain the objective.
2. Identify the files and components to change.
3. Implement the feature.
4. Run formatting.
5. Run relevant tests.
6. Run the build.
7. Fix failures introduced by the changes.
8. Update documentation.
9. Summarize what was completed and what remains.

Use the actual commands appropriate to the selected toolchain and repository configuration.

Do not claim a build or test passed unless it was actually run.

Do not create large amounts of placeholder code just to make the architecture look complete.

Prefer small, compiling, tested increments over broad speculative implementations.

When a design decision is uncertain, explain the tradeoffs and choose a reasonable default rather than blocking on minor questions.

For major architectural decisions, document the reasoning in an ADR or the architecture documentation.

---

# 12. Initial Milestones

Implement the project in the following order.

## Milestone 1: Application skeleton

Create a working Tauri + React + TypeScript application with a Rust backend.

Requirements:

- Cargo workspace.
- Frontend build and development workflow.
- Tauri application startup.
- Basic workbench layout.
- Logging.
- Initial documentation.
- A successful development build.

Do not implement the full extension system yet.

Deliverable: A working desktop application with a basic editor-oriented interface.

## Milestone 2: Text editing foundation

Implement the Rust text and editor core.

Requirements:

- Ropey-backed document storage.
- Document lifecycle.
- Basic text edits.
- Cursor and selection model.
- Undo and redo.
- Saving and dirty state.
- Unit tests.

Integrate the core with the frontend through a deliberately designed interface.

Deliverable: A functional text editor that opens, edits, and saves files.

## Milestone 3: Workbench basics

Implement:

- File explorer.
- Tabs.
- Command palette.
- Search and replace.
- Theme tokens.
- Persistent layout preferences.
- Basic keyboard shortcuts.

Deliverable: A usable programming editor rather than a bare text field.

## Milestone 4: Performance foundations

Implement:

- Efficient frontend updates.
- Viewport-oriented rendering.
- Large-file handling.
- Incremental document notifications.
- Background task scheduling.
- Performance instrumentation.
- Initial benchmarks.

Deliverable: A measurable performance baseline and a documented optimization strategy.

## Milestone 5: Syntax and language services

Implement Tree-sitter syntax highlighting and basic LSP integration.

Deliverable: A useful programming experience for at least one language.

## Milestone 6: WASM extension MVP

Implement:

- Manifest validation.
- WASM loading.
- Extension lifecycle.
- Versioned extension interface.
- Command registration.
- Document read access.
- Document-change notifications.
- Validated text edits.
- Diagnostics.
- Permissions.
- Resource limits.
- Example extension.
- Integration tests.

Deliverable: A working extension compiled separately to WASM that can add functionality to the running editor.

## Milestone 7: Customizable workbench

Implement the initial extension contribution model for:

- Themes.
- Keybindings.
- Menus.
- Status-bar items.
- Sidebar views.
- Editor decorations.

Deliverable: Extensions can customize the workbench's appearance and functionality without modifying the editor source code.

## Milestone 8: Hardening

Improve:

- Error handling.
- Crash recovery.
- Extension isolation.
- Resource accounting.
- Cross-platform behavior.
- Accessibility.
- Performance regression testing.
- Documentation.

Deliverable: A stable foundation for continued development.

---

# 13. Definition of Done

A milestone is complete only when its functionality works, relevant tests pass, and documentation is updated.

The initial project foundation is successful when:

1. The desktop application launches.
2. Files can be opened, edited, and saved.
3. Text operations are handled by the Rust core.
4. The frontend renders updates without repeatedly copying complete documents.
5. The application supports syntax highlighting.
6. Basic LSP integration works.
7. Performance benchmarks can be run reproducibly.
8. A separately compiled WASM extension can register and execute a command.
9. The extension can interact with documents only through the public API.
10. Extension permissions and resource limits are enforced.
11. Extensions can contribute to the UI through a host-managed mechanism.
12. The application remains responsive during background operations.
13. Failures in extensions do not ordinarily crash the editor.
14. The architecture and extension API are documented.

Do not treat every item as a prerequisite for the first commit. Use the milestone order above.

---

# 14. Final Instructions

Begin by inspecting the repository.

If it is empty, start with Milestone 1.

Do not attempt to implement the entire editor in a single pass.

Prioritize a working application, then build the editor core, establish performance measurements, and implement the extension system on top of stable interfaces.

Keep the architecture simple enough to evolve.

Use Rust for performance-sensitive core logic, Tauri for desktop integration, React for the workbench, and WASM for sandboxed extensions.

Treat memory efficiency, responsiveness, extension security, and UI customizability as core requirements rather than features to retrofit later.

At the end of each milestone, provide:

- A summary of the implementation.
- Important architectural decisions.
- Files and crates added or changed.
- Commands used to build and test.
- Actual test results.
- Known limitations.
- The next recommended milestone.

Start with Milestone 1 now.
