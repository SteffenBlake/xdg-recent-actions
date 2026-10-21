#!/bin/sh
set -eu
cd "$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
name=xdg-recent-actions
fail() { printf 'Install failed: %s\n' "$*" >&2; exit 1; }
[ "$(id -u)" != 0 ] || fail 'run as your desktop user, without sudo'
for cmd in cargo rustc cc systemctl systemd-analyze gio install mktemp flock sed; do
    command -v "$cmd" >/dev/null 2>&1 || fail "missing $cmd (Rust >= 1.89, a C linker, systemd user manager, and GLib tools are required)"
done
systemd_version=$(systemctl --version | sed -n '1s/^systemd \([0-9][0-9]*\).*/\1/p')
case "$systemd_version" in ''|*[!0-9]*) fail 'could not determine systemd version';; esac
[ "$systemd_version" -ge 250 ] || fail "systemd 250+ is required (found $systemd_version)"
systemctl --user show-environment >/dev/null 2>&1 || fail 'no reachable systemd user manager; run in your login session'
config=${XDG_CONFIG_HOME:-"$HOME/.config"}
data=${XDG_DATA_HOME:-"$HOME/.local/share"}
state=${XDG_STATE_HOME:-"$HOME/.local/state"}
# Keep the executable in the standard per-user location, independent of PATH.
bindir=$HOME/.local/bin
units=$config/systemd/user
for path in "$HOME" "$config" "$data" "$state"; do
    case "$path" in /*) ;; *) fail "path must be absolute: $path";; esac
    case "$path" in *'
'*|*\\*) fail 'newlines and backslashes in installation paths are unsupported';; esac
done
mkdir -p "$bindir" "$units" "$state/$name" "$data/applications"
chmod 700 "$state/$name"
# Serialize installers separately from the runtime lock, which sync takes itself.
exec 9>"$state/$name/install.lock"
flock -n 9 || fail 'another install or uninstall is running'
if [ -e "$bindir/$name" ]; then
    "$bindir/$name" --version
    printf 'Upgrade/reinstall: replacing %s and its user units. Desktop backups will be retained.\n' "$bindir/$name"
else
    printf 'New user installation: %s\n' "$bindir/$name"
fi
printf 'Building and testing before touching the running installation...\n'
cargo build --release --locked
cargo test --locked
./target/release/$name --version
legacy=0
if [ -f "$state/$name/state.json" ]; then
    [ -x "$bindir/$name" ] || fail 'an older state file exists but its installed binary is missing; restore that installation first'
    XDG_DATA_HOME="$data" XDG_STATE_HOME="$state" "$bindir/$name" check
    legacy=1
    printf 'Transition: the installed binary will restore its entries before the symlink layout is applied.\n'
else
    ./target/release/$name check
fi
stage=$(mktemp -d "$state/$name/install.XXXXXX")
cleanup() { rm -rf "$stage"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' HUP TERM
# Escape systemd specifiers, environment expansion, and quoted string syntax.
quote() { printf '%s' "$1" | sed 's/%/%%/g; s/"/\\"/g'; }
qbin=$(quote "$(pwd)/target/release/$name" | sed 's/\$/$$/g')
qdata=$(quote "$data")
qwatch=$(printf '%s' "$data/recently-used.xbel" | sed 's/%/%%/g')
qdirs=$(quote "${XDG_DATA_DIRS:-/usr/local/share:/usr/share}")
cat > "$stage/$name.service" <<UNIT
[Unit]
Description=Refresh Desktop Actions from XBEL recent history
StartLimitIntervalSec=0

[Service]
Type=oneshot
ExecStart="$qbin" sync
Environment="XDG_DATA_HOME=$qdata"
Environment="XDG_DATA_DIRS=$qdirs"
UMask=0077
NoNewPrivileges=yes
UNIT
cat > "$stage/$name.path" <<UNIT
[Unit]
Description=Watch XBEL recent history for Desktop Actions

[Path]
PathChanged=$qwatch
TriggerLimitIntervalSec=0
Unit=$name.service

[Install]
WantedBy=default.target
UNIT
systemd-analyze --user verify "$stage/$name.service" "$stage/$name.path"
# The verified unit differs only in its final executable path.
qbin=$(quote "$bindir/$name" | sed 's/\$/$$/g')
# Regenerate only ExecStart; use a literal line reader to avoid sed replacement syntax.
while IFS= read -r line; do
    case "$line" in ExecStart=*) printf 'ExecStart="%s" sync\n' "$qbin";; *) printf '%s\n' "$line";; esac
done < "$stage/$name.service" > "$stage/final.service"
mv "$stage/final.service" "$stage/$name.service"
was_active=$(systemctl --user is-active "$name.path" 2>/dev/null || :)
was_enabled=$(systemctl --user is-enabled "$name.path" 2>/dev/null || :)
for file in "$bindir/$name" "$units/$name.service" "$units/$name.path"; do
    if [ -e "$file" ]; then cp -p "$file" "$stage/$(basename "$file").old"; fi
done
rollback() {
    printf 'Activation failed; restoring the previous binary and units.\n' >&2
    systemctl --user stop "$name.path" "$name.service" >/dev/null 2>&1 || :
    restored=1
    if ! XDG_DATA_HOME="$data" ./target/release/$name restore; then
        printf 'Some replacement entries could not be restored; backups were retained and monitoring will stay stopped.\n' >&2
        restored=0
    fi
    for file in "$bindir/$name" "$units/$name.service" "$units/$name.path"; do
        old=$stage/$(basename "$file").old
        if [ -f "$old" ]; then cp -p "$old" "$file"; else rm -f "$file"; fi
    done
    systemctl --user daemon-reload || :
    if [ "$was_enabled" != enabled ]; then systemctl --user disable "$name.path" >/dev/null 2>&1 || :; fi
    if [ "$restored" = 1 ] && [ -x "$bindir/$name" ]; then
        XDG_DATA_HOME="$data" XDG_STATE_HOME="$state" "$bindir/$name" sync || restored=0
    fi
    if [ "$was_active" = active ] && [ "$restored" = 1 ]; then systemctl --user start "$name.path" || :; fi
}
trap 'rollback; cleanup; exit 130' INT
trap 'rollback; cleanup; exit 143' HUP TERM
systemctl --user stop "$name.path" "$name.service" >/dev/null 2>&1 || :
# Stage beside destinations so rename never exposes a partial executable or unit.
if ! (
    if [ "$legacy" = 1 ]; then
        XDG_DATA_HOME="$data" XDG_STATE_HOME="$state" "$bindir/$name" restore || exit 1
        ./target/release/$name check || exit 1
    fi
    install -m755 "target/release/$name" "$bindir/.$name.new" || exit 1
    mv -f "$bindir/.$name.new" "$bindir/$name" || exit 1
    for kind in service path; do
        install -m644 "$stage/$name.$kind" "$units/.$name.$kind.new" || exit 1
        mv -f "$units/.$name.$kind.new" "$units/$name.$kind" || exit 1
    done
    systemctl --user daemon-reload || exit 1
    systemctl --user reset-failed "$name.service" "$name.path" >/dev/null 2>&1 || :
    # Start monitoring before initial sync so a concurrent history change isn't missed.
    systemctl --user enable --now "$name.path" || exit 1
    systemctl --user start "$name.service" || exit 1
    systemctl --user is-active --quiet "$name.path"
); then rollback; fail 'installation rolled back; inspect journalctl --user -u xdg-recent-actions.service'; fi
trap 'exit 130' INT
trap 'exit 143' HUP TERM
if [ "$legacy" = 1 ]; then
    rm -f "$state/$name/state.json" "$state/$name/lock"
    rmdir "$state/$name/entries" 2>/dev/null || :
fi
printf '\nInstalled %s\nWatcher: active (no polling).\nUnits: %s\nGenerated files and originals: %s/%s\n' "$name" "$units" "$data" "$name"
if [ ! -f "$data/recently-used.xbel" ]; then printf 'History does not exist yet; the watcher will detect its creation.\n'; fi
printf 'Logs: journalctl --user -u %s.service\nUninstall: make uninstall\n' "$name"
