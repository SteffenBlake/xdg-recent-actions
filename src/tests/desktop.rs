use super::*;

#[test]
fn preserves_existing_sections_and_handles_action_collisions() {
    let text = "[Desktop Entry]\nName=Test\nType=Application\nExec=test %U\nActions=XdgRecent0;\n\n[Desktop Action XdgRecent0]\nName=Existing\nExec=test\n";
    let r = Recent {
        uri: "file:///tmp/a%20b".into(),
        path: "/tmp/a b".into(),
    };
    let out = render(
        text,
        &[r],
        Path::new("/tmp/test.desktop"),
        Path::new("/usr/bin/gio"),
    )
    .unwrap();
    assert!(out.contains("Actions=XdgRecent0;XdgRecent1;"));
    assert!(out.contains("Name=Existing\nExec=test"));
    assert!(out.contains("file:///tmp/a%%20b"));
    assert_eq!(exec_arg("a\"$`\\%"), "\"a\\\\\"\\\\$\\\\`\\\\\\\\%%\"");
}
