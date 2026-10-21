# xdg-recent-actions

Adds recent files and folders to application menus using freedesktop Desktop
Actions. Reads `recently-used.xbel` and writes user-local `.desktop` overrides.
Works with launchers that display Desktop Actions, independent of the desktop
environment or display server.

## Install

Requires Linux, a running systemd user session (systemd 250+), Rust 1.89+, Cargo,
a C linker, Make, `flock`, and GLib's `gio` command. Cargo downloads build
dependencies on the first build. Run as your desktop user, without sudo:

```sh
git clone https://github.com/SteffenBlake/xdg-recent-actions.git
cd xdg-recent-actions
make install
```

The installer checks prerequisites, builds and tests the program, checks the
history and existing overrides, then installs the binary and user units. It
runs an initial sync and enables the watcher for future logins. The build and
checks finish before the running installation is stopped. If activation fails,
the previous binary and units are restored.

To update an installation:

```sh
git pull --ff-only
make install
```

Reinstalling prints the old and new versions and replaces the binary and units.
Existing desktop-entry backups are retained.

## What it changes

A systemd path unit watches `$XDG_DATA_HOME/recently-used.xbel` using inotify.
After a write closes or the file is replaced, it starts one Rust process. There
is no polling or resident helper. Each run indexes installed applications once
and scans the XML through a buffered reader. It retains at most eight 32-byte
bookmark pins per matched application, then rereads those bookmark headers.
Desktop-entry text and recovery records are loaded one application at a time.
Unchanged output is detected with an exact byte comparison and is not rewritten.

The program adds up to eight recent items per application, newest first. It
matches recorded launch executables against installed entries, using application
names and desktop IDs to resolve duplicates. Ambiguous and unmatched records
are skipped. Only existing local files and directories are included. XBEL launch
commands are used for matching; actions launch through the installed desktop
entry using `gio launch`.

System-installed entries stay untouched. Overrides keep existing actions and
incorporate vendor-entry updates on the next sync. Existing user overrides are
backed up first. A manually edited generated file causes an error and is left
untouched. To customize an entry, uninstall first, edit the original, then
reinstall. Recovery state is journaled before each desktop-entry replacement.

Default locations (the usual XDG environment overrides are respected):

- Binary: `~/.local/bin/xdg-recent-actions`
- Units: `~/.config/systemd/user/xdg-recent-actions.{path,service}`
- Overrides: `~/.local/share/applications/`
- Ownership index: `~/.local/state/xdg-recent-actions/state.json`
- Per-app backups: `~/.local/state/xdg-recent-actions/entries/`

Keep the state directory while overrides are installed. Its per-app records
contain the original user entries needed for restoration. Older state is
migrated automatically during installation; preflight checks leave it untouched.

Applications must record history in XBEL for it to appear here. This program
does not read private application history databases. A launcher's own cache may
require reopening its menu before updated actions appear. Changes to installed
applications are incorporated at the next history change or manual sync.

## Inspect or run manually

```sh
~/.local/bin/xdg-recent-actions check
~/.local/bin/xdg-recent-actions sync
systemctl --user status xdg-recent-actions.path
journalctl --user -u xdg-recent-actions.service
```

`check` previews updates without writing files. Each run reports its elapsed
time and the number of unmatched or ambiguous records.

## Uninstall

```sh
make uninstall
```

Stops monitoring, restores original user entries, removes generated overrides,
and removes the binary and units. If a generated file was edited manually,
restoration stops with an error and leaves the installation available for
recovery, with monitoring stopped. The state directory is retained.

## Development

```sh
make test
make check
```

`make check` requires rustfmt and Clippy. An additional GIO activation test uses
`desktop-file-validate` and `python3-gi`; run it with
`cargo test --locked -- --include-ignored`. The optional benchmark harness also uses Python 3 and GNU time.

To compare release builds without touching installed entries:

```sh
python3 tools/benchmark.py --variant before=/path/to/old-binary --variant after=target/release/xdg-recent-actions --output results.json
```

It snapshots the current history into temporary directories and also measures
synthetic histories and large desktop entries. Results include elapsed time and
peak resident memory. All generated files are removed afterward.
