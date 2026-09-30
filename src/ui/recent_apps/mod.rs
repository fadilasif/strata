// SPDX-License-Identifier: MIT

//! Recently-used application history for the Open With dialog.
//!
//! History is keyed by MIME type (folders share `inode/directory`), never by
//! file path, so launching an app on one file surfaces it for every file of
//! the same type. Each list holds desktop IDs ordered most-recent-first with
//! a monotonic sequence, so restarts and multi-type merges keep true recency
//! order without depending on wall-clock time. Display code shows a stored
//! entry only while it remains among the resolved candidates, so MIME-type
//! and URI-support constraints always apply; entries for uninstalled
//! applications are pruned when new usage is recorded.

use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub(super) const MAX_PER_TYPE: usize = 5;
const STATE_VERSION: u32 = 1;
const STATE_FILE: &str = "strata/recent-apps.toml";

#[cfg(test)]
mod tests;

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct State {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    types: HashMap<String, Vec<Entry>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Entry {
    app: String,
    seq: u64,
}

pub(super) fn history_path() -> PathBuf {
    gtk::glib::user_data_dir().join(STATE_FILE)
}

/// Records a successful launch of `app_id` for every selected type.
/// Failures are advisory: history must never break opening files.
pub(super) fn record(content_types: &[String], app_id: &str, known_ids: &HashSet<String>) {
    if app_id.is_empty() || content_types.is_empty() {
        return;
    }
    let mut state = load_from(&history_path());
    record_in(&mut state, content_types, app_id);
    prune_unknown(&mut state, known_ids);
    save_to(&history_path(), &mut state);
}

pub(super) fn load() -> State {
    load_from(&history_path())
}

fn load_from(path: &Path) -> State {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return State::default(),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "unable to read recent application history");
            return State::default();
        }
    };
    match toml::from_str::<State>(&contents) {
        Ok(state) if state.version == STATE_VERSION => state,
        Ok(_) => {
            tracing::warn!(path = %path.display(), "ignoring recent application history with an unsupported version");
            State::default()
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "ignoring malformed recent application history");
            State::default()
        }
    }
}

fn save_to(path: &Path, state: &mut State) {
    state.version = STATE_VERSION;
    let contents = match toml::to_string(&*state) {
        Ok(contents) => contents,
        Err(error) => {
            tracing::warn!(%error, "unable to serialize recent application history");
            return;
        }
    };
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(path = %path.display(), %error, "unable to create recent application history directory");
        return;
    }
    if let Err(error) = crate::storage::atomic_write(path, contents.as_bytes()) {
        tracing::warn!(path = %path.display(), %error, "unable to save recent application history");
    }
}

fn record_in(state: &mut State, content_types: &[String], app_id: &str) {
    let base = next_seq(state);
    for (offset, content_type) in content_types.iter().enumerate() {
        let entries = state.types.entry(content_type.clone()).or_default();
        entries.retain(|entry| entry.app != app_id);
        entries.insert(
            0,
            Entry {
                app: app_id.to_owned(),
                seq: base + offset as u64,
            },
        );
        entries.truncate(MAX_PER_TYPE);
    }
}

fn next_seq(state: &State) -> u64 {
    state
        .types
        .values()
        .flatten()
        .map(|entry| entry.seq)
        .max()
        .map_or(1, |seq| seq + 1)
}

/// Drops entries for applications with no installed desktop file. Entries for
/// installed but currently incompatible applications are kept: they stay
/// valid for other types and must survive transient association changes. The
/// empty-set guard keeps a broken application database from wiping history.
fn prune_unknown(state: &mut State, known_ids: &HashSet<String>) {
    if known_ids.is_empty() {
        return;
    }
    state.types.retain(|_, entries| {
        entries.retain(|entry| known_ids.contains(entry.app.as_str()));
        !entries.is_empty()
    });
}

impl State {
    /// Desktop IDs for the selected types merged most-recent-first.
    pub(super) fn recent_ids(&self, content_types: &[String]) -> Vec<String> {
        let mut ordered: Vec<(&str, u64)> = Vec::new();
        for content_type in content_types {
            if let Some(entries) = self.types.get(content_type) {
                ordered.extend(entries.iter().map(|entry| (entry.app.as_str(), entry.seq)));
            }
        }
        ordered.sort_by_key(|entry| Reverse(entry.1));
        let mut seen = HashSet::new();
        ordered
            .into_iter()
            .filter(|(app, _)| seen.insert(*app))
            .map(|(app, _)| app.to_owned())
            .collect()
    }
}

/// Keeps the caller-provided order, dropping IDs outside the eligible set.
/// Eligibility (MIME compatibility, multi-type intersection, URI support) is
/// decided by the caller from resolved candidates, so this stays total.
pub(super) fn select_recent(ordered_ids: &[String], eligible_ids: &HashSet<String>) -> Vec<String> {
    ordered_ids
        .iter()
        .filter(|id| eligible_ids.contains(id.as_str()))
        .cloned()
        .collect()
}
