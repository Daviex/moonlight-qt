# GitHub build and release workflow

Updated 2026-09-06 on `codex/per-game-streaming-settings`.

The [Build workflow](../.github/workflows/build.yml) runs the existing Linux
AppImage, Steam Link, Windows and macOS builds. A successful **branch push** or
**manual dispatch** publishes a GitHub prerelease after all build jobs succeed.
Pull requests and tag pushes do not publish releases. Failed or cancelled builds
do not publish a new release.

## Published files

| Platform | Release asset |
| --- | --- |
| Windows x64 and ARM64 installer | `MoonlightSetup-<commit>.exe` |
| Windows x64 ZIP | `MoonlightPortable-x64-<commit>.zip` |
| Windows ARM64 ZIP | `MoonlightPortable-arm64-<commit>.zip` |
| macOS universal | `Moonlight-<commit>.dmg` |
| Linux x86_64 | `Moonlight-<commit>-x86_64.AppImage` |
| Steam Link | `Moonlight-SteamLink-<commit>.zip` |
| Windows and macOS debug symbols | `zDevDbgSyms-<OS>-<commit>.zip` |
| Package checksums | `SHA256SUMS` |

The Windows build already compiled both architectures and produced the installer
and ZIPs. The previous upload steps exposed deployment folders and omitted the
installer. They now upload the generated ZIPs unchanged and the combined setup
executable. CI ZIPs retain the existing `portable.dat.inactive` behavior: they use
the regular profile storage location until that marker is renamed.

Qt installation now explicitly uses `runner.temp` for both Windows kits, with
matching build paths, replacing the undefined `runner.workspace` expression.
The action's `dir` input adds a `Qt` directory below that location, as specified
in the [install-qt-action documentation](https://github.com/jurplel/install-qt-action/tree/v4#dir).

## Publication behavior

The [reusable publication workflow](../.github/workflows/prerelease-builds.yml)
downloads binaries and debug symbols from the current workflow run, checks that
every required artifact is present and nonempty, packages directory artifacts,
and produces SHA-256 checksums. Windows ZIPs, the installer, DMG and AppImage are
attached directly. Existing archives are not wrapped in another ZIP.

New releases are created as drafts with their assets, then published only after
the upload succeeds. Rerunning publication reuses the same tag and updates assets
with `--clobber`; it also completes an interrupted draft. If updating an already
published release fails midway, it may contain a mix of old and refreshed assets
until the rerun finishes. It is not an atomic replacement of an existing release.

Development releases are prereleases and do not replace the Latest stable release.
For a non-master branch, the name includes its full branch name, and the tag uses
the existing sanitized branch slug plus the six-character commit version. Master
keeps the existing `prerelease-<commit>` tag convention. These naming rules reuse
the previous publisher. A link to the resulting release is written in the run
summary. Draft/upload/publish operations use the existing `gh release` commands;
see the [GitHub CLI release reference](https://cli.github.com/manual/gh_release_create).

Only the publication job has `contents: write`; build jobs retain read access.
No new secret is required: publication uses the run's `GITHUB_TOKEN`.

The manual Run workflow button becomes available when the `workflow_dispatch`
declaration exists on the repository's default branch. Once available, the branch
can be selected when dispatching. Ordinary pushes work on this feature branch
without that merge. This default-branch requirement is documented in
[GitHub's workflow event reference](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#workflow_dispatch).

## Verification

The [local workflow tests](../tests/workflows/README.md) execute the actual asset
preparation and publication scripts against generated fixtures and a fake GitHub
CLI. These validate packaging and publication sequencing, not compilation of
Windows/macOS/Linux/Steam Link or a real release upload. Full build results must
come from the GitHub workflow itself.

The earlier cancelled run for the controller fix remains cancelled. The new
workflow commit is intended to run normally and publish when all builds succeed.
