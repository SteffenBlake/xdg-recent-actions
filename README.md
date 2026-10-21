# xdg-recent-actions

Adds recent files and folders to application menus using freedesktop Desktop
Actions. Reads `recently-used.xbel` and generates `.desktop` files for launchers
that display those actions.

## Install

Requires Linux, a running systemd user session (systemd 250+), Rust 1.89+, Cargo,
a C linker, Make, `flock`, and GLib's `gio` command. Cargo downloads build
dependencies on the first build. Run as your desktop user, without sudo:

```sh
git clone https://github.com/SteffenBlake/xdg-recent-actions.git
cd xdg-recent-actions
make install
```

The installer checks prerequisites, builds and tests the program, validates the
units, runs an initial sync, and enables monitoring for future logins. Build and
preflight failures leave the running installation alone. Activation failures
restore the previous binary and units.

To update:

```sh
git pull --ff-only
make install
```

Reinstalling prints the old and new versions before replacing the binary and
units.

## Files and ownership

Default layout, under `~/.local/share/`:

```text
applications/example.desktop -> ../xdg-recent-actions/generated/example.desktop
xdg-recent-actions/generated/example.desktop
xdg-recent-actions/originals/example.desktop
```

The symlink identifies an entry owned by this program. The original file exists
only if a user entry was already present. It is backed up once and restored on
uninstall. System-installed entries stay untouched.

Each update uses the saved original or the installed vendor entry as its source.
Generated output is never backed up as an original. If someone replaces the
symlink with a different file, that replacement is left alone and any original
backup is retained. Generated files are automatic output; to customize an entry,
uninstall, edit the original, then reinstall.

The program respects `XDG_DATA_HOME` and `XDG_DATA_DIRS`. It installs its binary at
`~/.local/bin/xdg-recent-actions` and its units under
`$XDG_CONFIG_HOME/systemd/user` (normally `~/.config/systemd/user`).

## Processing

A systemd path unit watches `recently-used.xbel` using inotify. After a write
closes or the file is replaced, it starts one Rust process. There is no polling
or resident helper.

The XML is scanned through a buffered reader. At most eight 32-byte bookmark
pins are retained per matched application. After validating the document, the
program rereads those headers and processes one desktop file at a time. BLAKE3
checksums detect unchanged output, which is not rewritten. Checksums are not
stored on disk.

Application names, desktop IDs, and launch executables are matched automatically.
Ambiguous and unmatched records are skipped. Only existing local files and
folders are included. Actions launch through the desktop entry using `gio launch`.
Applications must record history in XBEL; private application history databases
are not read. Vendor changes are picked up on the next sync. A launcher's cache
may require reopening its menu before updated actions appear.

## Commands

```sh
~/.local/bin/xdg-recent-actions check
~/.local/bin/xdg-recent-actions sync
systemctl --user status xdg-recent-actions.path
journalctl --user -u xdg-recent-actions.service
make uninstall
```

`check` previews changes without writing files. Uninstall stops monitoring,
restores originals, and removes generated files, symlinks, the binary, and units.
It refuses to overwrite a replacement entry.

## Development

```sh
make test
make check
make docs
```

Source uses Rust doc comments (`///` for items, `//!` for modules), shown on
hover in rust-analyzer. `make docs` generates browsable documentation in
`target/doc/xdg_recent_actions/index.html`.

`make check` requires rustfmt and Clippy. The additional GIO activation test uses
`desktop-file-validate` and `python3-gi`:

```sh
cargo test --locked -- --include-ignored
```

The benchmark harness requires Python 3 and GNU time. It works in temporary
folders and measures elapsed time and peak resident memory:

```sh
python3 tools/benchmark.py --variant current=target/release/xdg-recent-actions --output /tmp/xdg-recent-actions-bench.json
```
