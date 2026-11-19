//! Lossless GEDCOM reading, editing and writing.
//!
//! [`node`] is the raw line tree (keeps every tag, vendor extensions included); [`Tree`] is the
//! family-tree view and edit API over it; [`search`] is accent-insensitive person search.
pub mod date;
pub mod edit;
pub mod insights;
pub mod kinship;
pub mod merge;
pub mod node;
pub mod place_links;
pub mod places;
pub mod quality;
pub mod search;
pub mod tree;

pub use node::{Node, ParseError};
pub use date::{DateValue, Precision, normalize_date};
pub use edit::{EventEdit, PersonEdit};
pub use insights::{Completeness, Missing, TimelineItem};
pub use kinship::{KinAlt, KinKind, Kinship, Link};
pub use place_links::{Branch, Couple, Migration, MoveDir, ParentChild, PlaceLinks};
pub use places::{PlaceEvent, PlaceGroup, parse_coord, place_key};
pub use quality::{Issue, IssueKind, Severity};
pub use search::{MergeResults, PaletteResults, PersonHit, PlaceHit, SearchFilter, fold, merge_search, palette_search, search};
pub use tree::{EditError, Event, Family, Person, Sex, Tree, year_of};

/// Read a GEDCOM file. UTF-8 (with or without BOM) is expected; invalid bytes are replaced
/// rather than rejected so a slightly broken export still opens.
pub fn load(path: &std::path::Path) -> std::io::Result<Result<Tree, ParseError>> {
    let bytes = std::fs::read(path)?;
    Ok(Tree::parse(&String::from_utf8_lossy(&bytes)))
}

/// Atomic-ish save: write a sibling temp file, then rename over the target.
pub fn save(tree: &Tree, path: &std::path::Path) -> std::io::Result<()> {
    let tmp = path.with_extension("ged.tmp");
    std::fs::write(&tmp, tree.to_gedcom())?;
    std::fs::rename(&tmp, path)
}

/// Serializes tests that switch the global UI locale.
#[cfg(test)]
pub(crate) fn locale_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}
