//! Streaming XBEL scanning with bounded history per application. Retained records
//! are byte offsets, not XML trees or copies of the history. Positional reads
//! resolve the selected records after the complete document has been validated.

use crate::Result;
use quick_xml::{Reader, events::Event};
use std::{fs::File, io::BufRead, os::unix::fs::FileExt, path::PathBuf};
use url::Url;
const LIMIT: usize = 8;

/// One selected local bookmark, materialized only for the application being rendered.
#[derive(Clone, Debug)]
pub(crate) struct Recent {
    pub(crate) uri: String,
    pub(crate) path: PathBuf,
}

/// Location and ranking of a bookmark opening tag; no history text is retained.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pin {
    /// Byte offset of the bookmark opening tag in the scanned file.
    pub(crate) offset: u64,
    /// Byte length of that opening tag, excluding its child elements.
    pub(crate) length: u64,
    /// Application timestamp in microseconds; unknown timestamps rank last.
    pub(crate) modified: i64,
    /// URI fingerprint used for ranking and duplicate candidates.
    pub(crate) identity: u64,
}
/// Validate and scan XBEL, emitting borrowed URI, application name, command, and pin.
/// Only existing local files are emitted. Errors abort the pass before publication.
pub(crate) fn bookmarks(
    input: impl BufRead,
    mut emit: impl FnMut(&str, &str, &str, Pin) -> Result<()>,
) -> Result<()> {
    let mut reader = Reader::from_reader(input);
    let mut buffer = Vec::with_capacity(4096);
    let mut uri = None;
    let mut depth = 0usize;
    let mut root_seen = false;
    loop {
        let offset = reader.buffer_position();
        let event = reader.read_event_into(&mut buffer)?;
        let length = reader.buffer_position() - offset;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let local = e.local_name();
                if depth == 0 {
                    if local.as_ref() != b"xbel" || root_seen {
                        return Err("expected one XBEL root".into());
                    }
                    root_seen = true;
                }
                let mut href = std::borrow::Cow::Borrowed("");
                let mut name = std::borrow::Cow::Borrowed("");
                let mut exec = std::borrow::Cow::Borrowed("");
                let mut modified = std::borrow::Cow::Borrowed("");
                for attr in e.attributes() {
                    let attr = attr?;
                    let value = attr.decode_and_unescape_value(reader.decoder())?;
                    match attr.key.local_name().as_ref() {
                        b"href" => href = value,
                        b"name" => name = value,
                        b"exec" => exec = value,
                        b"modified" => modified = value,
                        _ => {}
                    }
                }
                if local.as_ref() == b"bookmark" {
                    uri = Url::parse(&href)
                        .ok()
                        .filter(|url| url.scheme() == "file")
                        .filter(|url| url.to_file_path().is_ok_and(|path| path.exists()))
                        .map(|url| (url.to_string(), offset, length));
                }
                if local.as_ref() == b"application"
                    && let Some((uri, offset, length)) = &uri
                {
                    let modified = chrono::DateTime::parse_from_rfc3339(&modified)
                        .ok()
                        .map(|t| t.timestamp_micros())
                        .unwrap_or(i64::MIN);
                    let pin = Pin {
                        offset: *offset,
                        length: *length,
                        modified,
                        identity: fingerprint(uri),
                    };
                    emit(uri, &name, &exec, pin)?;
                }
                if !empty {
                    depth += 1;
                } else if local.as_ref() == b"bookmark" {
                    uri = None;
                }
            }
            Event::End(e) => {
                depth = depth.checked_sub(1).ok_or("invalid XML depth")?;
                if e.local_name().as_ref() == b"bookmark" {
                    uri = None;
                }
            }
            Event::Text(t) if depth == 0 && !t.decode()?.trim().is_empty() => {
                return Err("text outside XBEL root".into());
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if !root_seen || depth != 0 {
        return Err("incomplete XBEL document; leaving actions unchanged".into());
    }
    Ok(())
}

/// Hash a URI for compact ranking and duplicate candidates; equality is checked on reread.
fn fingerprint(uri: &str) -> u64 {
    uri.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
/// Read one opening tag at its saved offset without moving the scanner file position.
/// Reject a record whose URI no longer matches the pin.
pub(crate) fn read_pin(file: &File, pin: Pin) -> Result<Recent> {
    let mut bytes = vec![0; usize::try_from(pin.length)?];
    file.read_exact_at(&mut bytes, pin.offset)?;
    let mut reader = Reader::from_reader(bytes.as_slice());
    let event = reader.read_event()?;
    let element = match event {
        Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"bookmark" => e,
        _ => return Err("history changed during scan".into()),
    };
    let mut uri = None;
    for attr in element.attributes() {
        let attr = attr?;
        if attr.key.local_name().as_ref() == b"href" {
            uri = Some(Url::parse(
                &attr.decode_and_unescape_value(reader.decoder())?,
            )?);
        }
    }
    let url = uri.ok_or("bookmark has no href")?;
    let path = url
        .to_file_path()
        .map_err(|_| "bookmark is no longer a local file")?;
    let uri = url.to_string();
    if fingerprint(&uri) != pin.identity {
        return Err("history changed during scan".into());
    }
    Ok(Recent { uri, path })
}
/// Keep at most eight newest distinct URIs, resolving hash collisions by rereading.
/// The group stays ordered by descending timestamp and then URI fingerprint.
pub(crate) fn retain_pin(file: &File, group: &mut Vec<Pin>, uri: &str, pin: Pin) -> Result<()> {
    // Hashes limit positional reads; compare actual URIs before treating a hash match as a duplicate.
    for i in 0..group.len() {
        if group[i].identity == pin.identity && read_pin(file, group[i])?.uri == uri {
            if group[i].modified >= pin.modified {
                return Ok(());
            }
            group.remove(i);
            break;
        }
    }
    let key = (std::cmp::Reverse(pin.modified), pin.identity);
    let position = group.partition_point(|p| (std::cmp::Reverse(p.modified), p.identity) <= key);
    if position >= LIMIT {
        return Ok(());
    }
    if group.len() == LIMIT {
        group.pop();
    }
    group.insert(position, pin);
    Ok(())
}
// Index both identities and executables once; never scan every app per bookmark.

#[cfg(test)]
#[path = "tests/history.rs"]
mod tests;
