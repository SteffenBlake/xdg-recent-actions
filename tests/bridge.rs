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

fn generated_file(f: &Fixture) -> PathBuf {
    f.path("data/xdg-recent-actions/generated/code.desktop")
}
fn original_file(f: &Fixture) -> PathBuf {
    f.path("data/xdg-recent-actions/originals/code.desktop")
}

#[test]
fn dry_run_cap_noop_stale_and_restore() {
    let f = Fixture::new();
    f.history(12);
    f.ok(&["check"]);
    assert!(!f.generated().exists());
    assert!(!f.path("data/xdg-recent-actions").exists());
    f.ok(&["sync"]);
    assert!(f.generated().is_symlink());
    assert_eq!(
        fs::read_link(f.generated()).unwrap(),
        Path::new("../xdg-recent-actions/generated/code.desktop")
    );
    let text = fs::read_to_string(f.generated()).unwrap();
    assert_eq!(text.matches("Name=Recent:").count(), 8);
    assert!(text.contains("Recent: file 11"));
    assert!(!text.contains("Recent: file 0 "));
    assert!(text.contains("Actions=New;XdgRecent0;"));
    let before = fs::metadata(generated_file(&f))
        .unwrap()
        .modified()
        .unwrap();
    let link_before = fs::symlink_metadata(f.generated())
        .unwrap()
        .modified()
        .unwrap();
    f.ok(&["sync"]);
    assert_eq!(
        before,
        fs::metadata(generated_file(&f))
            .unwrap()
            .modified()
            .unwrap()
    );
    assert_eq!(
        link_before,
        fs::symlink_metadata(f.generated())
            .unwrap()
            .modified()
            .unwrap()
    );
    assert!(!original_file(&f).exists());
    assert!(!f.path("state").exists());
    f.history(0);
    f.ok(&["sync"]);
    assert!(!f.generated().is_symlink());
    assert!(!generated_file(&f).exists());
    f.history(1);
    f.ok(&["sync"]);
    f.ok(&["restore"]);
    assert!(!f.generated().is_symlink());
}

#[test]
fn repeated_updates_preserve_original_once_and_do_not_stack_actions() {
    for user_override in [false, true] {
        let f = Fixture::new();
        let original = "[Desktop Entry]\nName=Mine\nType=Application\nExec=code %U\nActions=Custom;\n\n[Desktop Action Custom]\nName=Custom\nExec=code --new-window\n";
        if user_override {
            fs::write(f.generated(), original).unwrap();
            fs::set_permissions(f.generated(), fs::Permissions::from_mode(0o640)).unwrap();
        }
        let mut backup_mtime = None;
        for count in 1..=12 {
            f.history(count);
            f.ok(&["sync"]);
            if user_override {
                assert_eq!(fs::read_to_string(original_file(&f)).unwrap(), original);
                let mtime = fs::metadata(original_file(&f)).unwrap().modified().unwrap();
                if let Some(before) = backup_mtime {
                    assert_eq!(before, mtime);
                } else {
                    backup_mtime = Some(mtime);
                }
            } else {
                assert!(!original_file(&f).exists());
            }
            let text = fs::read_to_string(f.generated()).unwrap();
            assert_eq!(
                text.matches("# Generated by xdg-recent-actions;").count(),
                1
            );
            assert_eq!(text.matches("Name=Recent:").count(), count.min(8));
            if user_override {
                assert_eq!(text.matches("[Desktop Action Custom]").count(), 1);
            }
        }
        f.ok(&["restore"]);
        if user_override {
            assert_eq!(fs::read_to_string(f.generated()).unwrap(), original);
            assert_eq!(
                fs::metadata(f.generated()).unwrap().permissions().mode() & 0o777,
                0o640
            );
        } else {
            assert!(!f.generated().exists());
        }
    }
}

#[test]
fn replaced_user_entry_is_left_untouched_and_backup_is_retained() {
    let f = Fixture::new();
    f.history(1);
    let original = "[Desktop Entry]\nName=Mine\nType=Application\nExec=code %U\n";
    fs::write(f.generated(), original).unwrap();
    f.ok(&["sync"]);
    fs::remove_file(f.generated()).unwrap();
    fs::write(
        f.generated(),
        "[Desktop Entry]\nName=Changed\nType=Application\nExec=code %U\n",
    )
    .unwrap();
    assert!(!f.run(&["sync"]).status.success());
    assert!(!f.run(&["restore"]).status.success());
    assert!(
        fs::read_to_string(f.generated())
            .unwrap()
            .contains("Name=Changed")
    );
    assert_eq!(fs::read_to_string(original_file(&f)).unwrap(), original);
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
    fs::write(
        &vendor,
        fs::read_to_string(&vendor)
            .unwrap()
            .replace("Name=Code", "Name=Updated Code"),
    )
    .unwrap();
    f.ok(&["sync"]);
    assert!(
        fs::read_to_string(f.generated())
            .unwrap()
            .contains("Name=Updated Code")
    );
}

#[test]
fn interrupted_link_publication_is_repaired_without_backing_up_generated_content() {
    let f = Fixture::new();
    f.history(1);
    let original = "[Desktop Entry]\nName=Mine\nType=Application\nExec=code %U\n";
    fs::write(f.generated(), original).unwrap();
    f.ok(&["sync"]);
    fs::remove_file(f.generated()).unwrap();
    fs::write(f.generated(), original).unwrap();
    f.ok(&["sync"]);
    assert!(f.generated().is_symlink());
    assert_eq!(fs::read_to_string(original_file(&f)).unwrap(), original);
    fs::remove_file(f.generated()).unwrap();
    f.ok(&["sync"]);
    assert!(f.generated().is_symlink());
    fs::remove_file(generated_file(&f)).unwrap();
    f.ok(&["sync"]);
    assert!(generated_file(&f).exists());
    f.ok(&["restore"]);
    assert_eq!(fs::read_to_string(f.generated()).unwrap(), original);
}

#[test]
fn ambiguous_executables_and_unowned_symlinks_are_skipped() {
    let f = Fixture::new();
    f.history(1);
    fs::write(
        f.path("system/applications/other.desktop"),
        "[Desktop Entry]\nType=Application\nName=Other\nExec=code %U\n",
    )
    .unwrap();
    let history = f.path("data/recently-used.xbel");
    let xml = fs::read_to_string(&history).unwrap();
    fs::write(&history, xml.replace("name=\"code\"", "name=\"unknown\"")).unwrap();
    f.ok(&["sync"]);
    assert!(!f.generated().exists());
    fs::write(history, xml).unwrap();
    std::os::unix::fs::symlink(f.path("system/applications/code.desktop"), f.generated()).unwrap();
    assert!(!f.run(&["sync"]).status.success());
    assert_eq!(
        fs::read_link(f.generated()).unwrap(),
        f.path("system/applications/code.desktop")
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
fn duplicate_data_roots_never_treat_generated_entries_as_vendor_sources() {
    let f = Fixture::new();
    f.history(1);
    f.ok(&["sync"]);
    for _ in 0..3 {
        let output = Command::new(env!("CARGO_BIN_EXE_xdg-recent-actions"))
            .env("XDG_DATA_HOME", f.path("data"))
            .env(
                "XDG_DATA_DIRS",
                format!(
                    "{}:{}",
                    f.path("data").display(),
                    f.path("system").display()
                ),
            )
            .arg("sync")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(f.generated())
                .unwrap()
                .matches("Name=Recent:")
                .count(),
            1
        );
        assert!(!original_file(&f).exists());
    }
}

#[test]
fn cli_help_and_invalid_arguments_do_not_touch_user_data() {
    let f = Fixture::new();
    for args in [vec!["--help"], vec!["sync", "--help"], vec!["--version"]] {
        let output = f.ok(&args);
        assert!(!output.stdout.is_empty());
    }
    for args in [
        vec!["unknown"],
        vec!["restore", "--dry-run"],
        vec!["sync", "--unknown"],
    ] {
        let output = f.run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(!output.stderr.is_empty());
    }
    assert!(!f.path("data/xdg-recent-actions").exists());
    assert!(!f.generated().exists());
}
