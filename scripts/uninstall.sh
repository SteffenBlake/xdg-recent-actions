#!/bin/sh
set -eu
name=xdg-recent-actions
[ "$(id -u)" != 0 ] || { echo 'Run without sudo.' >&2; exit 1; }
state=${XDG_STATE_HOME:-"$HOME/.local/state"}/$name
units=${XDG_CONFIG_HOME:-"$HOME/.config"}/systemd/user
binary=$HOME/.local/bin/$name
mkdir -p "$state"
exec 9>"$state/install.lock"
flock -n 9 || { echo 'Another install/uninstall is running.' >&2; exit 1; }
printf 'Stopping monitoring and restoring original desktop entries...\n'
systemctl --user stop "$name.path" "$name.service" 2>/dev/null || :
# The unit environment is authoritative if XDG settings have since changed.
if [ -f "$units/$name.service" ]; then
    # Run restore through systemd with the same environment as the sync service.
    mkdir -p "$units/$name.service.d"
    printf '[Service]\nExecStart=\nExecStart="%s" restore\n' "$(printf '%s' "$binary" | sed 's/%/%%/g; s/\$/$$/g; s/"/\\"/g')" > "$units/$name.service.d/uninstall.conf"
    systemctl --user daemon-reload
    if ! systemctl --user start "$name.service"; then
        rm "$units/$name.service.d/uninstall.conf"
        systemctl --user daemon-reload
        echo 'Restore refused a manually edited file. Installation retained; watcher stopped. Check the service journal.' >&2
        exit 1
    fi
    rm "$units/$name.service.d/uninstall.conf"
    rmdir "$units/$name.service.d" 2>/dev/null || :
elif [ -x "$binary" ]; then "$binary" restore; fi
systemctl --user disable "$name.path" 2>/dev/null || :
rm -f "$units/$name.path" "$units/$name.service" "$binary"
systemctl --user daemon-reload
systemctl --user reset-failed "$name.service" "$name.path" 2>/dev/null || :
printf 'Uninstalled. Original entries restored. Recovery state retained at %s\n' "$state"
