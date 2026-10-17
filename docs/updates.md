# Updates

Open **More → Updates** to check the latest stable release from
[GreyforgeLabs/codepet on GitHub](https://github.com/GreyforgeLabs/codepet/releases/latest).
If a newer version is available, **Update** downloads, checks and installs it. No
terminal, GitHub sign-in or administrator password is needed. Checks happen when
you open this page or press **Check again**; there is no idle update daemon and no
automatic installation without an Update click.

Finish or discard running work and any pending review or application recovery,
then close the Work window. A saved suspected bug can stay: it is preserved across
the update. CodePet refuses to update while owned work or a Work window is active.
The work launch and update paths share the same lock, so a new job/window cannot
slip between that check and installation. The updater never kills a coding job.

The existing installer preserves the pet's life, age, XP, care, settings, saved
work and optional app connections. It retains its usual binary/runtime/plugin
recovery backups. A desktop pet that was visible returns after installation; the
bar plugin is reloaded without restarting the shell. The update runs as one
bounded user-systemd service, so closing the page or reloading the plugin does not
interrupt it. The restored desktop runs in its own user service, whose lifetime
follows its child processes; finishing the updater cannot kill it. Reopen Updates
to see the saved result.

## Requirements and limits

- Python **3.11 or later**, the standard library, and a working user-systemd session (systemd 250 or later).
- The release archive supports **x86_64 Linux**. The v0.4.0 binary needs **glibc
  2.39 or later**, Omarchy 4, Quickshell and Qt WebP support as described in the
  [README](../README.md#requirements). The updater does not install system packages.
- At least **512 MiB** free space to prepare the archive. Installer recovery copies
  can require additional space proportional to the previous installed runtime.
- Numeric stable versions only: drafts, prereleases, malformed releases and
  downgrades are refused. A newer local build is kept.

The check uses GitHub's unauthenticated latest-release API, under a 30-second
network-operation deadline. Rate-limit and network failures are shown in the page.
Installing checks the release again, then launches one service with a ten-minute
maximum lifetime, a 512 MiB memory cap and a 64-task cap. No API keys or GitHub
credentials are read. No source repository or pet data is uploaded by the updater.

Only the exact CodePet repository's versioned Linux archive and `SHA256SUMS` URLs
are accepted. HTTPS redirects are restricted to GitHub and its release-asset CDN.
The updater checks the archive against the one matching checksum entry and also
checks GitHub's SHA-256 asset digest when it is supplied. This relies on GitHub and
the repository's release publishing account; it is not an independent signing key.

Downloads are capped at 128 MiB for the archive and 64 KiB for checksums. Extraction
is capped at 256 MiB of member data and 12,000 entries; the decompressed tar stream
has an additional 16 MiB allowance for headers and metadata. Absolute/traversing paths, duplicates,
symlinks, hardlinks, devices, FIFOs and special permission bits are refused before
any installer runs. The archive must contain the expected version and installer
layout. The staged installer runs with the user's ordinary permissions.

## Failure and recovery

A bad download or unsafe archive changes no installed files. An interrupted or
failed installation is **not** silently resumed or declared successful just because
the binary version changed. Updates shows an error; inspect the private
`$XDG_STATE_HOME/codepet/update/install.log` (default
`~/.local/state/codepet/update/install.log`) and the installer's recovery backups
before trying again. A failed migration leaves pet data untouched and the log
contains the installer's recovery commands. The previous successful binary and
runtime backups live under `~/.cache/codepet` by default.

The latest status, one job's manifest/helper and installer log are private to the
user. Disposable download/extraction files are removed when the worker exits
normally, including handled failures. An interrupted service may leave its staging
until the next explicit attempt, which replaces that previous job's staging after
observing the old job terminal. It never automatically relaunches an unknown job.

For a terminal or diagnostic integration, the installed helper accepts:

```sh
codepet-update status   # local durable status only
codepet-update check    # GitHub release metadata only
codepet-update install  # explicit installation in an independent service
```

Each command prints a single `codepet.update.v1` JSON object with `current`,
`latest`, `state`, `message` and `checked_at`. Errors exit nonzero. The portable
Updates screen treats all messages as plain text and polls local status only while
an update is active and the screen is open. The helper refuses installation in a
`CODEPET_TEST_STATE=1` UI session. Tests inject an offline network object into the
Python module; there is no production URL override.
