use super::*;
use std::{collections::BTreeMap, fs, io::BufReader};

#[test]
fn bounded_pins_match_full_sort_with_unsorted_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let item = dir.path().join("a & b");
    fs::write(&item, "").unwrap();
    let mut xml = String::from("<xbel>");
    let mut newest = BTreeMap::new();
    for n in 0..10_000 {
        let mut url = Url::from_file_path(&item).unwrap();
        url.set_fragment(Some(&format!("item{}", (n * 7919) % 137)));
        let uri = url.to_string();
        let modified = (n * 3571) % 997;
        newest
            .entry(uri.clone())
            .and_modify(|t: &mut i64| *t = (*t).max(modified))
            .or_insert(modified);
        let stamp = chrono::DateTime::from_timestamp(modified, 0)
            .unwrap()
            .to_rfc3339();
        xml.push_str(&format!(
            "<bookmark href=\"{}\"><application modified=\"{stamp}\"/></bookmark>",
            quick_xml::escape::escape(&uri)
        ));
    }
    xml.push_str("</xbel>");
    let history = dir.path().join("history");
    fs::write(&history, &xml).unwrap();
    let file = File::open(history).unwrap();
    let mut retained = Vec::new();
    bookmarks(BufReader::new(&file), |uri, _, _, pin| {
        retain_pin(&file, &mut retained, uri, pin)?;
        assert!(retained.len() <= LIMIT);
        Ok(())
    })
    .unwrap();
    let mut expected: Vec<_> = newest.into_iter().collect();
    expected.sort_by_key(|(uri, modified)| (std::cmp::Reverse(*modified), fingerprint(uri)));
    expected.truncate(LIMIT);
    let result = retained
        .into_iter()
        .map(|pin| {
            let r = read_pin(&file, pin).unwrap();
            (r.uri, pin.modified / 1_000_000)
        })
        .collect::<Vec<_>>();
    assert_eq!(result, expected);
    assert_eq!(std::mem::size_of::<Pin>(), 32);
}
#[test]
fn malformed_xml_does_not_clear_history() {
    assert!(bookmarks("<xbel><bookmark>".as_bytes(), |_, _, _, _| Ok(())).is_err());
    assert!(bookmarks("not xml".as_bytes(), |_, _, _, _| Ok(())).is_err());
    assert!(bookmarks("<wrong/>".as_bytes(), |_, _, _, _| Ok(())).is_err());
    assert!(
        bookmarks("<xbel/>".as_bytes(), |_, _, _, _| panic!(
            "unexpected record"
        ))
        .is_ok()
    );
}
