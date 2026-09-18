// SPDX-License-Identifier: MIT

//! Header-only archive enumeration for Quick Look previews.
//!
//! Reads member names, directory flags and sizes without decoding file data,
//! so a preview can present a virtual tree without materializing any content
//! on disk. Names are treated as opaque strings; unresolved host paths are
//! never created (see `crate::services::archive_preview_tree`).
//!
//! Password-protected archives are surfaced through
//! [`ArchiveListingStatus`] rather than as errors: name-only listings stay
//! readable, and a supplied password is validated against the archive's own
//! decryption setup without touching any member data.

use crate::services::{ArchiveFileEntry, ArchiveFormat};
use std::{
    cell::Cell,
    fs::File,
    io::{Read, Seek},
    path::Path,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

#[cfg(test)]
mod tests;

pub(crate) const INVALID_ARCHIVE: &str = "This file is not a valid archive or is damaged.";

/// State 8: the parser recognized a well-formed archive but hit a feature,
/// method, or scheme it does not implement. Distinct from [`INVALID_ARCHIVE`]
/// (state 5): the file is fine, this app cannot preview it.
pub(crate) const ARCHIVE_UNSUPPORTED_MESSAGE: &str =
    "This archive format is not supported for preview.";

/// State 6: the sandboxed renderer could not finish, without exposing
/// sandbox/renderer internals. Covers renderer failure and timeout.
pub(crate) const ARCHIVE_PREVIEW_FAILED_MESSAGE: &str = "Preview couldn't be completed. Try again.";

/// Preview-only safety limits for archive listing (extraction is unaffected).
pub(crate) const MAX_ARCHIVE_ENTRIES: usize = 20_000;
pub(crate) const MAX_TAR_GZ_COMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;
pub(crate) const MAX_TAR_GZ_DECOMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;
pub(crate) const ARCHIVE_TOO_LARGE_MESSAGE: &str = "Archive too large to preview.";

/// Upper bound accepted for a staged archive password.
pub(crate) const MAX_ARCHIVE_PASSWORD_BYTES: usize = 4096;

/// Wire statuses for the sandboxed archive-listing JSON contract.
const WIRE_OPEN: &str = "open";
const WIRE_NEEDS_PASSWORD: &str = "needs-password";
const WIRE_WRONG_PASSWORD: &str = "wrong-password";
const WIRE_UNSUPPORTED: &str = "unsupported";
const WIRE_TOO_LARGE: &str = "too-large";
const WIRE_ERROR: &str = "error";

/// Whether the listed archive needs a password and whether one was accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArchiveListingStatus {
    /// Every member is readable without a password.
    Open,
    /// The archive is encrypted; a password is required but none was supplied.
    NeedsPassword,
    /// A password was supplied but the archive rejected it.
    WrongPassword,
    /// The archive is protected in a way this build cannot handle.
    Unsupported,
}

/// The outcome of a header-only listing.
pub(crate) struct ArchiveListing {
    pub status: ArchiveListingStatus,
    pub entries: Vec<ArchiveFileEntry>,
}

impl ArchiveListing {
    fn open(entries: Vec<ArchiveFileEntry>) -> Self {
        Self {
            status: ArchiveListingStatus::Open,
            entries,
        }
    }
}

/// Lists the members of the archive at `archive_path`.
///
/// Runs with ambient authority: invoke only inside the sandbox helper.
/// The parent process must use the sandboxed archive-listing operation and
/// never call this directly.
///
/// Returns `Err` with a user-facing message when the file is not something the
/// selected format can read. `cancelled` is honored between members so a long
/// listing can still be aborted promptly. `password` is only consulted for
/// protected archives; unencrypted members are unaffected.
pub(crate) fn list_archive_entries_direct(
    archive_path: &Path,
    format: ArchiveFormat,
    password: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<ArchiveListing, String> {
    match format {
        ArchiveFormat::Zip => list_zip(archive_path, password, cancelled),
        ArchiveFormat::SevenZ => list_7z(archive_path, password, cancelled),
        ArchiveFormat::Tar => list_tar(archive_path, false, cancelled),
        ArchiveFormat::TarGz => list_tar(archive_path, true, cancelled),
        ArchiveFormat::Rar => Err(ARCHIVE_UNSUPPORTED_MESSAGE.to_owned()),
    }
}

/// Rejects another accepted entry once the preview entry cap is reached.
///
/// Counts only entries that passed filtering and would be pushed into the
/// listing. Like every other archive size/complexity limit, a capped listing
/// is a hard "too large" error, never a silent truncation.
fn ensure_entry_budget(accepted: usize) -> Result<(), String> {
    if accepted >= MAX_ARCHIVE_ENTRIES {
        return Err(ARCHIVE_TOO_LARGE_MESSAGE.to_owned());
    }
    Ok(())
}

/// Preview-only bounds on member names. A single ~32 MiB `a/a/...` name once
/// forced a ~6 GiB tree into the parent (`archive_preview_tree`), and a wide,
/// long-shared-prefix listing stalled its main thread: both stayed under the
/// entry and output caps. These limits keep the tree's node and name-bytes
/// counts bounded. Exceeding one maps to the unified "too large" message, in
/// the helper and again in the parent decode.
const MAX_ARCHIVE_NAME_BYTES: usize = 16 * 1024;
const MAX_ARCHIVE_ENTRY_SEGMENTS: usize = 4096;
const MAX_ARCHIVE_TOTAL_NAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_ARCHIVE_TOTAL_SEGMENTS: usize = 512 * 1024;

/// Path components a name contributes to the preview tree.
///
/// Counts exactly what [`split_archive_name`] produces — `/`-separated,
/// non-empty components with `.`, `..` and `\` kept literal — so the budget
/// bounds the nodes the tree actually creates. Previously this dropped dot
/// segments while the tree resolves nothing; keeping them uncounted would let
/// dot-heavy names grow nodes no budget accounts for.
fn entry_name_segments(name: &str) -> usize {
    crate::services::split_archive_name(name).len()
}

/// Cumulative member-name budget enforced while a listing is collected
/// (helper) and again after a sandbox payload decodes (parent). Both layers
/// fail closed with the too-large message so no pathological name set ever
/// reaches `archive_preview_tree`.
#[derive(Default)]
struct MemberNameBudget {
    name_bytes: usize,
    segments: usize,
}

impl MemberNameBudget {
    fn check(&mut self, name: &str) -> Result<(), String> {
        if name.len() > MAX_ARCHIVE_NAME_BYTES
            || self.name_bytes.saturating_add(name.len()) > MAX_ARCHIVE_TOTAL_NAME_BYTES
        {
            return Err(ARCHIVE_TOO_LARGE_MESSAGE.to_owned());
        }
        let segments = entry_name_segments(name);
        if segments > MAX_ARCHIVE_ENTRY_SEGMENTS
            || self.segments.saturating_add(segments) > MAX_ARCHIVE_TOTAL_SEGMENTS
        {
            return Err(ARCHIVE_TOO_LARGE_MESSAGE.to_owned());
        }
        self.name_bytes += name.len();
        self.segments += segments;
        Ok(())
    }
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        Err("Preview cancelled".to_owned())
    } else {
        Ok(())
    }
}

/// Sorts ZIP failures into the Quick Look error contract: only genuinely
/// unimplemented archive features stay unsupported (state 8); every other
/// failure — corruption, truncation, I/O — is invalid-or-corrupt (state 5).
/// Dependency wording never reaches the caller either way. Password outcomes
/// never pass through here: raw access skips validation, and the
/// password-retry call matches its outcomes first.
fn zip_error(error: zip::result::ZipError) -> String {
    use zip::result::ZipError;
    match error {
        ZipError::UnsupportedArchive(_) | ZipError::CompressionMethodNotSupported(_) => {
            ARCHIVE_UNSUPPORTED_MESSAGE.to_owned()
        }
        _ => INVALID_ARCHIVE.to_owned(),
    }
}

/// Sorts 7z header failures into the Quick Look error contract: only the
/// recognized unimplemented-feature case stays unsupported (state 8); every
/// other header failure — bad signature, checksum/CRC mismatch, truncated
/// structures, I/O, unrecognized versions or methods — means the archive
/// could not be parsed at all (state 5). Password outcomes never pass
/// through here: they match to statuses at the call site first.
fn sevenz_list_error(error: sevenz_rust2::Error) -> String {
    use sevenz_rust2::Error;
    match error {
        Error::Unsupported(_) => ARCHIVE_UNSUPPORTED_MESSAGE.to_owned(),
        _ => INVALID_ARCHIVE.to_owned(),
    }
}

fn list_zip(
    archive_path: &Path,
    password: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<ArchiveListing, String> {
    let file = File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(zip_error)?;
    // Bound the preallocation by the preview cap: a malicious central
    // directory can claim an absurd entry count.
    let mut entries = Vec::with_capacity(archive.len().min(MAX_ARCHIVE_ENTRIES));
    let mut first_encrypted = None;
    let mut first_encrypted_file = None;
    let mut budget = MemberNameBudget::default();
    for index in 0..archive.len() {
        check_cancelled(cancelled)?;
        // Raw access avoids the crypto reader, so encrypted members remain
        // enumerable from the central directory without a password.
        let member = archive.by_index_raw(index).map_err(zip_error)?;
        if member.encrypted() {
            first_encrypted.get_or_insert(index);
            if !member.is_dir() {
                first_encrypted_file.get_or_insert(index);
            }
        }
        ensure_entry_budget(entries.len())?;
        budget.check(member.name())?;
        entries.push(ArchiveFileEntry {
            name: member.name().to_owned(),
            directory: member.is_dir(),
            size: member.size(),
        });
    }
    let Some(index) = first_encrypted_file.or(first_encrypted) else {
        return Ok(ArchiveListing::open(entries));
    };
    let Some(password) = password else {
        return Ok(ArchiveListing {
            status: ArchiveListingStatus::NeedsPassword,
            entries,
        });
    };
    // Opening a protected member validates the decryption header immediately;
    // no member data is produced.
    match archive.by_index_with_options(
        index,
        zip::read::ZipReadOptions::new().password(Some(password.as_bytes())),
    ) {
        Ok(_) => Ok(ArchiveListing::open(entries)),
        Err(zip::result::ZipError::InvalidPassword) => Ok(ArchiveListing {
            status: ArchiveListingStatus::WrongPassword,
            entries,
        }),
        Err(zip::result::ZipError::UnsupportedArchive(_)) => Ok(ArchiveListing {
            status: ArchiveListingStatus::Unsupported,
            entries,
        }),
        Err(error) => Err(zip_error(error)),
    }
}

fn list_7z(
    archive_path: &Path,
    password: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<ArchiveListing, String> {
    let file = File::open(archive_path).map_err(|error| error.to_string())?;
    let password = password
        .map(sevenz_rust2::Password::new)
        .unwrap_or_else(sevenz_rust2::Password::empty);
    use sevenz_rust2::Error;
    let archive = match sevenz_rust2::ArchiveReader::new(file, password) {
        // A header that decrypts only with an empty password is promptable;
        // a supplied password that fails header decoding is wrong.
        Ok(archive) => archive,
        Err(Error::PasswordRequired) => {
            return Ok(ArchiveListing {
                status: ArchiveListingStatus::NeedsPassword,
                entries: Vec::new(),
            });
        }
        Err(Error::MaybeBadPassword(_)) => {
            return Ok(ArchiveListing {
                status: ArchiveListingStatus::WrongPassword,
                entries: Vec::new(),
            });
        }
        Err(Error::Unsupported(_)) => {
            return Ok(ArchiveListing {
                status: ArchiveListingStatus::Unsupported,
                entries: Vec::new(),
            });
        }
        Err(error) => return Err(sevenz_list_error(error)),
    };
    let mut entries = Vec::with_capacity(archive.archive().files.len().min(MAX_ARCHIVE_ENTRIES));
    let mut budget = MemberNameBudget::default();
    for member in &archive.archive().files {
        check_cancelled(cancelled)?;
        ensure_entry_budget(entries.len())?;
        budget.check(&member.name)?;
        entries.push(ArchiveFileEntry {
            name: member.name.clone(),
            directory: member.is_directory,
            size: member.size,
        });
    }
    // Content-only encryption (a plain header that decrypts without a
    // password) is not discoverable through this crate, so such archives are
    // listed here; this mirrors sevenz_rust2's own read-side limitation.
    Ok(ArchiveListing::open(entries))
}

fn list_tar(
    archive_path: &Path,
    gzip: bool,
    cancelled: &AtomicBool,
) -> Result<ArchiveListing, String> {
    let file = File::open(archive_path).map_err(|error| error.to_string())?;
    if gzip {
        // Preview-only compressed-size gate, checked before the decoder exists.
        if file.metadata().map(|metadata| metadata.len()).unwrap_or(0) > MAX_TAR_GZ_COMPRESSED_BYTES
        {
            return Err(ARCHIVE_TOO_LARGE_MESSAGE.to_owned());
        }
        let decoder = flate2::read::GzDecoder::new(file);
        let reader = BudgetReader {
            inner: decoder,
            remaining: MAX_TAR_GZ_DECOMPRESSED_BYTES,
            cancelled,
        };
        let mut archive = tar::Archive::new(reader);
        let entries = archive.entries().map_err(tar_list_error)?;
        collect_tar_members(entries, cancelled)
    } else {
        // A plain tar is read from a seekable `File`, so unread member data is
        // skipped with a real seek instead of being read and discarded.
        let file_len = file
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or(u64::MAX);
        let furthest_seek = Rc::new(Cell::new(0u64));
        let tracker = SeekTargetTracker {
            inner: file,
            furthest: furthest_seek.clone(),
        };
        let listing = collect_tar_seekable(tracker, cancelled)?;
        if furthest_seek.get() > file_len {
            return Err(INVALID_ARCHIVE.to_owned());
        }
        Ok(listing)
    }
}

/// Caps cumulative decompressed bytes read through a TAR.GZ stream.
///
/// No single read ever returns bytes beyond the remaining budget, and a read
/// attempted after the budget is exhausted fails unless the stream is truly
/// at EOF — so a stream ending exactly at the budget still reports a clean
/// end instead of a false over-budget error. Cancellation is checked on every
/// read, keeping skips over large members abortable. (Across the sandbox
/// boundary, cancellation additionally arrives as process termination from
/// the parent.)
struct BudgetReader<'a, R> {
    inner: R,
    remaining: u64,
    cancelled: &'a AtomicBool,
}

impl<R: Read> Read for BudgetReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("Preview cancelled"));
        }
        if self.remaining == 0 {
            let read = self.inner.read(buf)?;
            if read == 0 {
                return Ok(0);
            }
            return Err(std::io::Error::other(ARCHIVE_TOO_LARGE_MESSAGE));
        }
        let allowed = (buf.len() as u64).min(self.remaining) as usize;
        let read = self.inner.read(&mut buf[..allowed])?;
        self.remaining -= read as u64;
        Ok(read)
    }
}

/// Records the furthest stream position reached by seeking.
///
/// `entries_with_seek` skips unread member data with `Seek`, and seeking
/// past the end of the file succeeds silently: the following header read
/// then reports a clean end-of-archive. Tracking the furthest seek landing
/// lets the plain-tar path reject member data truncated past EOF without
/// reading the data back.
struct SeekTargetTracker<R> {
    inner: R,
    furthest: Rc<Cell<u64>>,
}

impl<R: Read> Read for SeekTargetTracker<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: Seek> Seek for SeekTargetTracker<R> {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        let position = self.inner.seek(position)?;
        self.furthest.set(self.furthest.get().max(position));
        Ok(position)
    }
}

/// Sorts TAR/GZIP stream failures into the Quick Look error contract. The
/// sandbox decompression budget and cancellation surface through this same
/// channel as io errors carrying Strata's own messages, so those pass
/// through untouched; every dependency failure means the stream could not be
/// parsed at all (state 5), and dependency wording never reaches the caller.
fn tar_list_error(error: std::io::Error) -> String {
    let message = error.to_string();
    if message == ARCHIVE_TOO_LARGE_MESSAGE || message == "Preview cancelled" {
        message
    } else {
        INVALID_ARCHIVE.to_owned()
    }
}

/// Lists a plain tar from a seekable reader.
///
/// `tar::Archive::entries_with_seek` advances past unread member data with
/// `Seek` rather than the read-and-discard loop that `entries()` uses, so
/// listing costs I/O proportional to the entry count instead of the total
/// byte size of the archive.
fn collect_tar_seekable<R: Read + Seek>(
    reader: R,
    cancelled: &AtomicBool,
) -> Result<ArchiveListing, String> {
    let mut archive = tar::Archive::new(reader);
    let entries = archive.entries_with_seek().map_err(tar_list_error)?;
    collect_tar_members(entries, cancelled)
}

fn collect_tar_members<'a, R: Read + 'a>(
    entries: tar::Entries<'a, R>,
    cancelled: &AtomicBool,
) -> Result<ArchiveListing, String> {
    let mut listed = Vec::new();
    let mut budget = MemberNameBudget::default();
    for member in entries {
        check_cancelled(cancelled)?;
        let member = member.map_err(tar_list_error)?;
        if matches!(
            member.header().entry_type(),
            tar::EntryType::XGlobalHeader
                | tar::EntryType::XHeader
                | tar::EntryType::GNULongName
                | tar::EntryType::GNULongLink
        ) {
            continue;
        }
        let name = member.path().map_err(tar_list_error)?;
        let directory = member.header().entry_type().is_dir();
        if directory && name == Path::new(".") {
            continue;
        }
        let name = name.to_string_lossy().into_owned();
        budget.check(&name)?;
        ensure_entry_budget(listed.len())?;
        listed.push(ArchiveFileEntry {
            name,
            directory,
            size: member.size(),
        });
    }
    // tar has no archive-level encryption: the whole archive is always listed.
    Ok(ArchiveListing::open(listed))
}

/// Serializes a listing result across the sandbox boundary (helper side).
///
/// Every outcome the parent must distinguish — including the preview safety
/// limits — has a distinct wire status, so no partial listing ever crosses.
pub(crate) fn encode_archive_result(result: &Result<ArchiveListing, String>) -> Vec<u8> {
    let (status, entries, message): (&str, &[ArchiveFileEntry], Option<&str>) = match result {
        Ok(listing) => (
            match listing.status {
                ArchiveListingStatus::Open => WIRE_OPEN,
                ArchiveListingStatus::NeedsPassword => WIRE_NEEDS_PASSWORD,
                ArchiveListingStatus::WrongPassword => WIRE_WRONG_PASSWORD,
                ArchiveListingStatus::Unsupported => WIRE_UNSUPPORTED,
            },
            &listing.entries,
            None,
        ),
        Err(message) if message == ARCHIVE_TOO_LARGE_MESSAGE => (WIRE_TOO_LARGE, &[], None),
        Err(message) => (WIRE_ERROR, &[], Some(message.as_str())),
    };
    let entries: Vec<serde_json::Value> = entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "name": entry.name,
                "directory": entry.directory,
                "size": entry.size,
            })
        })
        .collect();
    serde_json::json!({
        "status": status,
        "entries": entries,
        "message": message,
    })
    .to_string()
    .into_bytes()
}

fn invalid_archive() -> String {
    INVALID_ARCHIVE.to_owned()
}

/// Wire statuses for the sandboxed archive-listing JSON contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WireStatus {
    Open,
    NeedsPassword,
    WrongPassword,
    Unsupported,
    TooLarge,
    Error,
}

impl WireStatus {
    fn parse(status: &str) -> Option<Self> {
        match status {
            WIRE_OPEN => Some(Self::Open),
            WIRE_NEEDS_PASSWORD => Some(Self::NeedsPassword),
            WIRE_WRONG_PASSWORD => Some(Self::WrongPassword),
            WIRE_UNSUPPORTED => Some(Self::Unsupported),
            WIRE_TOO_LARGE => Some(Self::TooLarge),
            WIRE_ERROR => Some(Self::Error),
            _ => None,
        }
    }
}

/// A structurally validated archive-listing payload from the sandbox helper.
///
/// Every known status validates here — including the ones
/// `decode_archive_listing` maps to errors. A well-formed limit or error
/// payload is valid output, not invalid output: the output-validity gate must
/// never swallow a legitimate `too-large` result into "invalid image data".
struct ArchivePayload {
    pub status: WireStatus,
    pub entries: Vec<ArchiveFileEntry>,
    pub message: Option<String>,
}

/// Validates the shape of a sandboxed listing payload without mapping its
/// status to success or failure.
fn decode_archive_payload(data: &[u8]) -> Result<ArchivePayload, String> {
    let value: serde_json::Value = serde_json::from_slice(data).map_err(|_| invalid_archive())?;
    let status = value
        .get("status")
        .and_then(serde_json::Value::as_str)
        .and_then(WireStatus::parse)
        .ok_or_else(invalid_archive)?;
    let entries = value
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(invalid_archive)?;
    if entries.len() > MAX_ARCHIVE_ENTRIES {
        return Err(invalid_archive());
    }
    let mut listed = Vec::with_capacity(entries.len().min(MAX_ARCHIVE_ENTRIES));
    for entry in entries {
        let name = entry
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(invalid_archive)?;
        let directory = entry
            .get("directory")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(invalid_archive)?;
        let size = entry
            .get("size")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(invalid_archive)?;
        listed.push(ArchiveFileEntry {
            name: name.to_owned(),
            directory,
            size,
        });
    }
    let message = value
        .get("message")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    if status == WireStatus::Error && message.as_deref().is_none_or(str::is_empty) {
        return Err(invalid_archive());
    }
    Ok(ArchivePayload {
        status,
        entries: listed,
        message,
    })
}

/// Structural payload check for the sandbox output-validity gate.
pub(crate) fn archive_payload_valid(data: &[u8]) -> bool {
    decode_archive_payload(data).is_ok()
}

/// Validates and decodes a sandboxed listing payload (parent side).
///
/// Anything that is not exactly the contract above — wrong shapes, unknown
/// statuses, more entries than the preview cap allows — fails closed with the
/// invalid-archive message rather than a partial or untrusted listing. Names
/// outside the member-name budget also fail closed, but as "too large": such
/// payloads stay structurally valid output for the sandbox gate, so the
/// distinction still surfaces as the size limit instead of "invalid archive".
pub(crate) fn decode_archive_listing(data: &[u8]) -> Result<ArchiveListing, String> {
    let payload = decode_archive_payload(data)?;
    let mut budget = MemberNameBudget::default();
    for entry in &payload.entries {
        budget.check(&entry.name)?;
    }
    match payload.status {
        WireStatus::Open => Ok(ArchiveListing::open(payload.entries)),
        WireStatus::NeedsPassword => Ok(ArchiveListing {
            status: ArchiveListingStatus::NeedsPassword,
            entries: payload.entries,
        }),
        WireStatus::WrongPassword => Ok(ArchiveListing {
            status: ArchiveListingStatus::WrongPassword,
            entries: payload.entries,
        }),
        WireStatus::Unsupported => Ok(ArchiveListing {
            status: ArchiveListingStatus::Unsupported,
            entries: payload.entries,
        }),
        WireStatus::TooLarge => Err(ARCHIVE_TOO_LARGE_MESSAGE.to_owned()),
        WireStatus::Error => Err(payload
            .message
            .filter(|message| !message.is_empty())
            .ok_or_else(invalid_archive)?),
    }
}
