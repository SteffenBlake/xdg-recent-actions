use super::*;
use std::path::PathBuf;

#[test]
fn matching_prefers_command_and_skips_ambiguity() {
    let app = |id: &str, exe: &str| App {
        id: id.into(),
        source: PathBuf::new(),
        name: id.into(),
        executable: exe.into(),
    };
    let apps = vec![
        app("nemo.desktop", "nemo"),
        app("inkscape.desktop", "inkscape"),
        app("other.desktop", "inkscape"),
    ];
    let index = Index::new(&apps);
    assert_eq!(index.find("nemo", "'inkscape %F'"), None);
    assert_eq!(index.find("inkscape", "'inkscape %F'"), Some(1));
    assert_eq!(index.find("inkscape", "'nemo %U'"), Some(0));
}
#[test]
fn generic_wrappers_require_an_application_identity() {
    let apps = vec![App {
        id: "org.example.App.desktop".into(),
        name: "Example".into(),
        executable: "flatpak".into(),
        source: PathBuf::new(),
    }];
    let index = Index::new(&apps);
    assert_eq!(index.find("unknown", "flatpak run org.other.App %u"), None);
    assert_eq!(
        index.find("org.example.App", "flatpak run org.example.App %u"),
        Some(0)
    );
}
