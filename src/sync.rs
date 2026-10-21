//! One synchronization pass: discover applications, scan history, then render and
//! publish one application at a time. No output changes before XML validation.

use crate::{
    Result,
    desktop::{inventory, render},
    history::{Pin, bookmarks, read_pin, retain_pin},
    matching::Index,
    paths::Paths,
    storage::{managed_ids, publish, restore_one},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::BufReader,
    path::Path,
};

/// Scan a stable history snapshot and update each application with its selected recents.
/// Detect in-place history changes before publication and remove obsolete owned actions.
pub(crate) fn run(paths: &Paths, dry: bool, gio: &Path) -> Result<()> {
    let apps = inventory(paths)?;
    let index = Index::new(&apps);
    let mut groups: BTreeMap<usize, Vec<Pin>> = BTreeMap::new();
    let mut skipped = 0;
    let file = match File::open(paths.data.join("recently-used.xbel")) {
        Ok(file) => Some(file),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    if let Some(file) = &file {
        let before = file.metadata()?;
        bookmarks(BufReader::new(file), |uri, name, exec, pin| {
            if let Some(i) = index.find(name, exec) {
                retain_pin(file, groups.entry(i).or_default(), uri, pin)?;
            } else {
                skipped += 1;
            }
            Ok(())
        })?;
        let after = file.metadata()?;
        if before.len() != after.len() || before.modified()? != after.modified()? {
            return Err("history changed during scan; leaving actions unchanged".into());
        }
    }
    let mut wanted = BTreeSet::new();
    let mut changed = 0;
    let mut conflicts = Vec::new();
    for (i, pins) in groups {
        let group = pins
            .into_iter()
            .map(|pin| {
                read_pin(
                    file.as_ref().expect("pins require an open history file"),
                    pin,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let app = &apps[i];
        wanted.insert(app.id.clone());
        let target = paths.target(&app.id);
        let text = fs::read_to_string(&app.source)?;
        let output = render(&text, &group, &target, gio)?;
        match publish(paths, &app.id, &output, dry) {
            Ok(true) => {
                changed += 1;
                println!(
                    "{} {} ({} recent items)",
                    if dry { "Would update" } else { "Updating" },
                    app.id,
                    group.len()
                );
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!("{e}");
                conflicts.push(app.id.clone());
            }
        }
    }
    for id in managed_ids(paths)? {
        if wanted.contains(&id) {
            continue;
        }
        println!("{} {id}", if dry { "Would restore" } else { "Restoring" });
        if let Err(e) = restore_one(paths, &id, dry) {
            eprintln!("{e}");
            conflicts.push(id);
        }
    }
    println!(
        "{} apps indexed; {changed} updates; {skipped} unmatched/ambiguous records skipped",
        apps.len()
    );
    if !conflicts.is_empty() {
        return Err(format!(
            "conflicting entries: {}; no conflicting target was overwritten",
            conflicts.join(", ")
        )
        .into());
    }
    Ok(())
}
