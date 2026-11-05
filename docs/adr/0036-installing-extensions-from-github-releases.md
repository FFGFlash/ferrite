# ADR 0036: Installing extensions from GitHub releases

- **Status:** Accepted
- **Date:** 2026-11-05

## Context

Extensions were installed by copying a folder into the extensions folder, and the official
extensions now live in repositories of their own
([ADR 0034](0034-official-extensions-in-their-own-repositories.md)), so there was no way to get
one but building it. A marketplace is a non-goal of the first release (`DESIGN.md`), but the
releases GitHub already hosts can carry built extensions. The user chose installing by
repository plus a list of the official extensions, with updates, uninstalling, and choosing a
release, and asked for the official repositories to publish their releases themselves.

## Decisions

### A release carries `extension.zip`

The package is the extension's folder, zipped, with `extension.toml` at its root (or in its one
top-level folder). One fixed asset name keeps it unambiguous, whatever else a release has.
Versions are tags (`v1.2.0`), which the official repositories' workflow requires to match the
manifest. Manifests carry only `major.minor.patch`: a prerelease is the tag with its name added
(`v1.2.0-alpha.1`) and the manifest unchanged. So an installed extension's version for updates is
the tag it came from, not its manifest's, or `v1.2.0` wouldn't count as newer than the alpha
before it.

### Checked before it's asked about, asked about before it's installed

The download is checked against the SHA-256 digest GitHub reports for the asset (over HTTPS from
the API, while the file comes from GitHub's storage), limited to 100 MiB, and unpacked to at most
256 MiB in 10,000 files, refusing links and paths outside the folder. The manifest is then
validated as an installed extension's is, with its entrypoint inside the package. Only then is the
user asked, in a native dialog that shows what the manifest says: name, version, author,
description, repository and release, and the permissions it requests. An extension still asks
before using those permissions; installing grants nothing.

There's no signature beyond GitHub's digest and HTTPS: whoever can publish a release in the
repository can publish an extension. Extensions are sandboxed and their permissions are asked for,
so this trusts the repository, not the code.

### Staged, then swapped

Packages unpack into `<config dir>/extension-staging`, on the same volume as the extensions
folder. Installing moves an older version aside, moves the new one in (into a folder named by
its id, or the folder a hand-copied one was in), and puts the old one back if that fails.
Uninstalling moves the folder out the same way before deleting it, so neither leaves half an
extension. `.flareite-install.json` in the folder records the repository, tag, and digest, for
updates.

### Uninstalling forgets the extension

Its permission decisions (`ExtensionHost::forget_decisions`) and stored values go with it, so
installing it again asks again. Its settings stay in `settings.toml`, which is the user's file.

### The catalog is a topic on GitHub

The Extensions view lists the extensions of the owners in `extensions.catalog` (`FlareiteLabs` by
default): their repositories with the `flareite-extension` topic, at their newest stable release,
with name, version, description, and author from that release's `extension.toml` (fetched from
`raw.githubusercontent.com`, which doesn't count toward the API's limit). Publishing an official
extension is tagging it; no list in this repository has to change. Anyone can list their own by
adding their owner to the setting.

### No network unasked

Requests are anonymous (60 an hour from one address), and happen only when the user installs,
checks for updates, or opens the Extensions view (the catalog, once a session, cached). Nothing
checks for updates in the background. `extensions.catalog = []` turns the list off.

### Libraries

`reqwest` was in the tree already (through Tauri) without TLS; it's used blocking, with rustls
and the operating system's certificates (`rustls-platform-verifier`), so proxies and corporate
roots work. `zip` reads packages, with only deflate (through `zlib-rs`, pure Rust). The logic is a
crate of its own (`flareite-extension-install`), tested against a local server that answers as
GitHub would; `src-tauri` only confirms and reloads.

## Consequences

- The official extensions need the `flareite-extension` topic to be listed, and a release from
  their workflow to be installable.
- Updating replaces the folder, so an extension's own files there don't survive an update; what
  it keeps belongs in its storage.
- An update that requests new permissions installs, and then waits for the user to allow them,
  as a newly installed extension does.
- Extensions published elsewhere than GitHub can only be copied in by hand.
