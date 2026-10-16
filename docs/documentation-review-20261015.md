# Documentation review — 2026-10-15

## Scope and evidence

The owner requested a project-wide documentation review, Web first and Desktop second,
with code findings presented for a decision. This is the Desktop record; the Web review is
in [shvia-web PR #567](https://github.com/samirhvbr/shvia-web/pull/567).

Implementation baseline: `master` at `983f2aa`, version 1.29.0, after `git pull --ff-only`.
The documentation delivery advances to **1.29.1** using the repository's required
synchronization flow. Only Markdown and version fields change. No application logic,
dependency version, credential, installed binary or server configuration changes.

Evidence comes from Rust/TypeScript source, bridge/runners, configuration, package and
workflow definitions, existing guides and source-level checks. No signing credentials,
user credential files or production environment were read. No real login, package build,
assessment or deployment is implied.

## Corrections and new coverage

| Finding | Documentation improvement |
|---|---|
| READMEs described June/0.4.5 as current and centered on an unbuilt Python sidecar | Reconciled English and Portuguese entrypoints with the Tauri/Rust shell, shared Web interface and actual engine paths |
| No compact map separated server features, installed capabilities and remote execution | Added [current product and capability map](project-state.md) |
| Architecture inverted version compatibility and said the updater did not exist | Documented the server-declared minimum Desktop warning and the implemented updater using the configured server |
| Native local state was reduced to a generic store/Keychain description | Documented server URL, browser state, profiles, bindings, folder grants, usage caches and assessment recovery/outboxes |
| Index called Codex bridge/UI and Run unimplemented after delivery | Updated current entries, marked measurement/design history and added missing usage/subscription guides |
| Account guide said macOS profiles and registration UI did not exist | Reconciled directory/credential profiles, explicit alias discovery and current account-selection/login boundaries |
| Earlier feature entries said reload always kills agents and absent engines are never installed | Marked these as historical behavior, superseded by resumable sessions and the later installation flow |
| Brand guide still presented Portal as current | Pointed to current Shield assets/provenance, preserving prior identity as history |
| Agent/build instructions described stale stack and incomplete validation | Reconciled the mirrored contracts and build guide with current commands, runtimes, CI, local packaging and distribution boundaries |
| Native subscription methods existed without their caller in the reviewed Web baseline | Recorded the cross-repository dependency in the handoff guide and map; native implementation is not a deployed end-to-end button |
| OS and lifecycle limits were scattered through implementation notes | Added a platform table and explicit distinctions among reload, background, window close, app restart and assessment recovery |

## Remaining coverage and acceptance work

The capability map covers source surfaces, not every possible interaction or all historical
prose. The stable feature register retains dated entries; a release that introduced a
capability is not evidence that every current platform was exercised.

- **Installed walkthroughs:** execute/update the Windows, Linux microphone/paste, Run,
  subscription-connect and controlled-assessment procedures against identified Web,
  Desktop, runner and runtime versions. Existing pending pilot notes remain pending.
- **Distribution:** verify actual artifacts, signatures, notarization/stapling, update
  manifests and installed upgrades in their own release process. This docs patch does not
  produce a replacement installer merely because version metadata advances.
- **Cross-device continuity:** local sessions are window-owned; remote engine continuity
  depends on Web/Workspace and a driving screen. Validate those paths separately instead
  of promising universal process recovery from a stored transcript.
- **Documentation automation:** freshness checks compare version claims and link checks
  measure discoverability/paths. Neither proves that a sentence still matches runtime
  behavior. A future feature-to-guide guard would need an explicit supported-feature
  inventory; none is implemented here.
- **Historical material:** design specifications and measurements retain dates and
  attribution. Open work remains in `.continue/`; it is not moved or deleted as cleanup.

## Code findings for owner decisions

**Adjacent backups can retain credentials.** In `src-tauri/src/cli_config.rs`, `gravar()`
copies a preexisting client config to a fixed `.shvia-bak` path before writing. It only
attempts `0600` for a newly created destination, and it does not harden existing
configurations or their backups. The supported destinations can carry API credentials.

This behavior is confirmed by source inspection of the current branch; no actual user
configuration or backup was opened and no new credential exposure was measured. Before
changing it, decide the recovery behavior: remove adjacent copies, use a private bounded
backup store or another explicit restoration mechanism, and cover Unix modes and Windows
ACL handling in the implementation review. The documentation change does not quietly
remove a user's recovery copy or alter their configuration.

**The macOS publish gate does not independently establish every notarization property.**
`build-local.sh`'s `confere_assinatura_apple()` verifies the `.app` with
`codesign --verify --deep --strict` and, when a DMG exists, validates its stapled ticket.
That gate does not check an app's stapled ticket or run `spctl`/Developer ID policy
validation. An app-only bundle may therefore pass this gate without that additional
publication evidence. This is a source-level candidate, not a reproduced distribution
failure. Reproduce the app-only and DMG paths in an appropriate signing environment,
then decide whether the gate should require those properties for each artifact type.
No signing or publication command was executed by this review.

The Web review separately records migration rollback, remote engine provenance, image
policy gates and the legacy file-quota setting. Those are Web/source candidates and are
not duplicated as Desktop defects.

## Validation and delivery boundaries

Use existing version/freshness/documentation checks, local path/discoverability inspection,
mirrored AGENTS/CLAUDE comparison and `git diff --check`. The actual delivery report states
which checks ran and their results; this page does not predeclare tests or CI green.

A reviewable PR is documentation delivery. Merge/release, a signed native package,
publication to the update channel and installed-client acceptance remain separate acts.
