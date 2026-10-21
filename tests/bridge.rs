use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;

struct Fixture {
    dir: TempDir,
}
impl Fixture {
    fn new() -> Self {
        let f = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::create_dir_all(f.path("data/applications")).unwrap();
        fs::create_dir_all(f.path("system/applications")).unwrap();
        fs::write(f.path("system/applications/code.desktop"), "[Desktop Entry]\nType=Application\nName=Code\nExec=code %U\nActions=New;\n\n[Desktop Action New]\nName=New\nExec=code --new-window\n").unwrap();
        f
    }
    fn path(&self, s: &str) -> PathBuf {
        self.dir.path().join(s)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_xdg-recent-actions"))
            .env("XDG_DATA_HOME", self.path("data"))
            .env("XDG_DATA_DIRS", self.path("system"))
            .env("XDG_STATE_HOME", self.path("state"))
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Output {
        let o = self.run(args);
        assert!(
            o.status.success(),
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        o
    }
    fn history(&self, count: usize) {
        let mut xml = String::from(
            "<xbel xmlns:b=\"http://www.freedesktop.org/standards/desktop-bookmarks\">",
        );
        for n in 0..count {
            let p = self.path(&format!("file {n}"));
            fs::write(&p, "").unwrap();
            xml.push_str(&format!("<bookmark href=\"{}\"><info><metadata><b:applications><b:application name=\"code\" exec=\"'code %u'\" modified=\"2026-01-{:02}T00:00:00Z\"/></b:applications></metadata></info></bookmark>", url::Url::from_file_path(p).unwrap(), n+1));
        }
        xml.push_str("</xbel>");
        fs::write(self.path("data/recently-used.xbel"), xml).unwrap();
    }
    fn generated(&self) -> PathBuf {
        self.path("data/applications/code.desktop")
    }
}

#[test]
fn dry_run_cap_noop_stale_and_restore() {
    let f = Fixture::new();
    f.history(12);
    f.ok(&["check"]);
    assert!(!f.generated().exists());
    assert!(!f.path("state").exists());
    f.ok(&["sync"]);
    let text = fs::read_to_string(f.generated()).unwrap();
    assert_eq!(text.matches("Name=Recent:").count(), 8);
    assert!(text.contains("Recent: file 11"));
    assert!(!text.contains("Recent: file 0 "));
    assert!(text.contains("Actions=New;XdgRecent0;"));
    let before = fs::metadata(f.generated()).unwrap().modified().unwrap();
    let state_before = fs::metadata(f.path("state/xdg-recent-actions/entries/code.desktop.json"))
        .unwrap()
        .modified()
        .unwrap();
    f.ok(&["sync"]);
    assert_eq!(
        before,
        fs::metadata(f.generated()).unwrap().modified().unwrap()
    );
    assert_eq!(
        state_before,
        fs::metadata(f.path("state/xdg-recent-actions/entries/code.desktop.json"))
            .unwrap()
            .modified()
            .unwrap()
    );
    f.history(0);
    f.ok(&["sync"]);
    assert!(!f.generated().exists());
    f.history(1);
    f.ok(&["sync"]);
    f.ok(&["restore"]);
    assert!(!f.generated().exists());
}
#[test]
fn user_override_is_restored_byte_for_byte_and_manual_edits_refused() {
    let f = Fixture::new();
    f.history(1);
    let original = "# mine\n[Desktop Entry]\nName=Personal Code\nType=Application\nExec=code %U\n";
    fs::write(f.generated(), original).unwrap();
    fs::set_permissions(f.generated(), fs::Permissions::from_mode(0o640)).unwrap();
    f.ok(&["sync"]);
    fs::write(f.generated(), "user changed this").unwrap();
    assert!(!f.run(&["sync"]).status.success());
    assert!(!f.run(&["restore"]).status.success());
    assert_eq!(
        fs::read_to_string(f.generated()).unwrap(),
        "user changed this"
    );
    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(f.path("state/xdg-recent-actions/entries/code.desktop.json")).unwrap(),
    )
    .unwrap();
    fs::write(f.generated(), state["generated"].as_str().unwrap()).unwrap();
    f.ok(&["restore"]);
    assert_eq!(fs::read_to_string(f.generated()).unwrap(), original);
    assert_eq!(
        fs::metadata(f.generated()).unwrap().permissions().mode() & 0o777,
        0o640
    );
}
#[test]
fn malformed_xml_preserves_outputs_and_vendor_updates_are_incorporated() {
    let f = Fixture::new();
    f.history(1);
    f.ok(&["sync"]);
    let before = fs::read(f.generated()).unwrap();
    fs::write(f.path("data/recently-used.xbel"), "<xbel><bookmark>").unwrap();
    assert!(!f.run(&["sync"]).status.success());
    assert_eq!(before, fs::read(f.generated()).unwrap());
    f.history(1);
    let vendor = f.path("system/applications/code.desktop");
    let new = fs::read_to_string(&vendor)
        .unwrap()
        .replace("Name=Code", "Name=Updated Code");
    fs::write(&vendor, new).unwrap();
    f.ok(&["sync"]);
    assert!(
        fs::read_to_string(f.generated())
            .unwrap()
            .contains("Name=Updated Code")
    );
}
#[test]
fn ambiguous_executables_are_skipped() {
    let f = Fixture::new();
    f.history(1);
    fs::write(
        f.path("system/applications/other.desktop"),
        "[Desktop Entry]\nType=Application\nName=Other\nExec=code %U\n",
    )
    .unwrap();
    let xml = fs::read_to_string(f.path("data/recently-used.xbel"))
        .unwrap()
        .replace("name=\"code\"", "name=\"unknown\"");
    fs::write(f.path("data/recently-used.xbel"), xml).unwrap();
    f.ok(&["sync"]);
    assert!(!f.generated().exists());
}
#[test]
fn journal_recovers_interrupted_first_write_and_replacement() {
    let f = Fixture::new();
    f.history(1);
    f.ok(&["sync"]);
    fs::remove_file(f.generated()).unwrap(); // Journal written, initial file never renamed.
    f.ok(&["sync"]);
    assert!(f.generated().exists());
    f.history(2);
    f.ok(&["sync"]);
    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(f.path("state/xdg-recent-actions/entries/code.desktop.json")).unwrap(),
    )
    .unwrap();
    fs::write(f.generated(), state["previous"].as_str().unwrap()).unwrap();
    f.ok(&["sync"]);
    assert_eq!(
        fs::read_to_string(f.generated())
            .unwrap()
            .matches("Name=Recent:")
            .count(),
        2
    );
}
#[test]
#[ignore = "GIO activation test requires desktop-file-validate and python3-gi"]
fn generated_exec_round_trips_special_characters_without_shell_execution() {
    let f = Fixture::new();
    let capture = f.path("capture");
    let executable = f.path("capture.sh");
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            capture.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        f.path("system/applications/code.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Code\nExec={} %U\n",
            executable.display()
        ),
    )
    .unwrap();
    let file = f.path("space % quote\" dollar$ tick` slash\\ &.txt");
    fs::write(&file, "").unwrap();
    let uri = url::Url::from_file_path(&file).unwrap().to_string();
    let xml = format!(
        "<xbel><bookmark href=\"{}\"><application name=\"code\" exec=\"'code %u'\"/></bookmark></xbel>",
        quick_xml::escape::escape(&uri)
    );
    fs::write(f.path("data/recently-used.xbel"), xml).unwrap();
    f.ok(&["sync"]);
    let o = Command::new("desktop-file-validate")
        .arg(f.generated())
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stdout));
    // Ask GIO itself to parse and activate the action, exercising both levels of escaping.
    let py = "import sys; from gi.repository import Gio; a=Gio.DesktopAppInfo.new_from_filename(sys.argv[1]); a.launch_action('XdgRecent0', None)";
    assert!(
        Command::new("/usr/bin/python3")
            .args(["-c", py])
            .arg(f.generated())
            .status()
            .unwrap()
            .success()
    );
    for _ in 0..100 {
        if capture.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let args = fs::read_to_string(capture).unwrap();
    assert!(
        args.trim() == uri || Path::new(args.trim()) == file,
        "unexpected argv: {args:?}"
    );
}

#[test]
fn legacy_state_migrates_and_can_be_exported_for_installer_rollback() {
    let f = Fixture::new();
    f.history(1);
    f.ok(&["sync"]);
    let entry = f.path("state/xdg-recent-actions/entries/code.desktop.json");
    let record: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&entry).unwrap()).unwrap();
    let legacy = serde_json::json!({"version":1,"entries":{"code.desktop":record}});
    let manifest = f.path("state/xdg-recent-actions/state.json");
    fs::write(&manifest, serde_json::to_vec(&legacy).unwrap()).unwrap();
    fs::remove_file(&entry).unwrap();
    let desktop_before = fs::read(f.generated()).unwrap();
    f.ok(&["check"]);
    assert!(!entry.exists()); // Preflight stays read-only even for older state formats.
    f.ok(&["sync"]);
    let v2: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    assert_eq!(v2["version"], 2);
    assert_eq!(v2["entries"]["code.desktop"], false);
    assert_eq!(desktop_before, fs::read(f.generated()).unwrap());
    f.ok(&["export-state-v1"]);
    let exported: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    assert_eq!(exported, legacy);
    f.ok(&["sync"]);
    f.ok(&["restore"]);
    assert!(!f.generated().exists());
}
