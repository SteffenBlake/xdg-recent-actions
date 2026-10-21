use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
};
use tempfile::TempDir;

struct Install {
    root: TempDir,
}
impl Install {
    fn new() -> Self {
        let f = Self {
            root: tempfile::tempdir().unwrap(),
        };
        for dir in ["scripts", "stubs", "target/release"] {
            fs::create_dir_all(f.path(dir)).unwrap();
        }
        fs::write(
            f.path("scripts/install.sh"),
            include_str!("../scripts/install.sh"),
        )
        .unwrap();
        fs::write(
            f.path("scripts/uninstall.sh"),
            include_str!("../scripts/uninstall.sh"),
        )
        .unwrap();
        f.script(
            "target/release/xdg-recent-actions",
            "#!/bin/sh\nprintf 'candidate %s\\n' \"$*\"\n",
        );
        f.script("stubs/id", "#!/bin/sh\necho 1000\n");
        f.script(
            "stubs/cargo",
            "#!/bin/sh\necho \"cargo $*\" >> \"$HOME/calls\"\n[ ! -f \"$HOME/fail-build\" ]\n",
        );
        f.script(
            "stubs/systemd-analyze",
            "#!/bin/sh\n[ ! -f \"$HOME/fail-verify\" ]\n",
        );
        f.script(
            "stubs/systemctl",
            r##"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/calls"
case "$*" in
 *--version*) echo "systemd 260";;
 *show-environment*) [ ! -f "$HOME/fail-manager" ];;
 *is-active*) [ -f "$HOME/active" ] && echo active;;
 *is-enabled*) [ -f "$HOME/enabled" ] && echo enabled;;
 *'enable --now'*) touch "$HOME/active" "$HOME/enabled";;
 *'stop '*) rm -f "$HOME/active";;
 *'start xdg-recent-actions.service'*)
   if [ -f "$HOME/fail-start" ]; then rm "$HOME/fail-start"; exit 1; fi;;
 *'start xdg-recent-actions.path'*) touch "$HOME/active";;
 *'disable '*) rm -f "$HOME/enabled";;
 *) exit 0;;
esac
"##,
        );
        f
    }
    fn path(&self, p: &str) -> PathBuf {
        self.root.path().join(p)
    }
    fn script(&self, p: &str, text: &str) {
        let p = self.path(p);
        fs::write(&p, text).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn run(&self, script: &str) -> Output {
        Command::new("sh")
            .arg(self.path(&format!("scripts/{script}.sh")))
            .current_dir(self.root.path())
            .env("HOME", self.root.path())
            .env("XDG_DATA_HOME", self.path("data"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.path("stubs").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .output()
            .unwrap()
    }
    fn installed(&self) -> PathBuf {
        self.path(".local/bin/xdg-recent-actions")
    }
    fn units(&self) -> PathBuf {
        self.path("config/systemd/user")
    }
}
fn success(o: Output) {
    assert!(
        o.status.success(),
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}
#[test]
fn install_reinstall_and_uninstall() {
    let f = Install::new();
    success(f.run("install"));
    assert!(f.installed().exists());
    assert!(f.path("active").exists());
    let o = f.run("install");
    assert!(String::from_utf8_lossy(&o.stdout).contains("Upgrade/reinstall"));
    success(o);
    success(f.run("uninstall"));
    assert!(!f.installed().exists());
    assert!(!f.units().join("xdg-recent-actions.path").exists());
}
#[test]
fn preflight_failures_leave_existing_installation_running() {
    let f = Install::new();
    success(f.run("install"));
    let original = fs::read(f.installed()).unwrap();
    for flag in ["fail-build", "fail-verify", "fail-manager"] {
        fs::write(f.path(flag), "").unwrap();
        assert!(!f.run("install").status.success());
        assert!(f.path("active").exists());
        assert_eq!(original, fs::read(f.installed()).unwrap());
        fs::remove_file(f.path(flag)).unwrap();
    }
}
#[test]
fn failed_activation_restores_old_binary_units_and_watcher() {
    let f = Install::new();
    success(f.run("install"));
    let before = fs::read(f.installed()).unwrap();
    let unit_before = fs::read(f.units().join("xdg-recent-actions.service")).unwrap();
    f.script(
        "target/release/xdg-recent-actions",
        "#!/bin/sh\necho upgraded\n",
    );
    fs::write(f.path("fail-start"), "").unwrap();
    let o = f.run("install");
    assert!(!o.status.success());
    assert_eq!(before, fs::read(f.installed()).unwrap());
    assert_eq!(
        unit_before,
        fs::read(f.units().join("xdg-recent-actions.service")).unwrap()
    );
    assert!(f.path("active").exists());
    assert!(f.path("enabled").exists());
}
#[test]
fn failed_first_activation_removes_partial_installation() {
    let f = Install::new();
    fs::write(f.path("fail-start"), "").unwrap();
    assert!(!f.run("install").status.success());
    assert!(!f.installed().exists());
    assert!(!f.path("active").exists());
    assert!(!f.path("enabled").exists());
}

#[test]
fn transition_restores_old_installation_and_removes_legacy_index() {
    let f = Install::new();
    success(f.run("install"));
    let legacy = f.path("state/xdg-recent-actions/state.json");
    fs::write(&legacy, "legacy fixture").unwrap();
    let output = f.run("install");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Transition:"));
    assert!(stdout.contains("candidate restore"));
    success(output);
    assert!(!legacy.exists());
    assert!(f.path("active").exists());
}
