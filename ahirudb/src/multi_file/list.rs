//! The rules a file list answers its questions with.
//!
//! Ported from `duckdb/common/multi_file/multi_file_list.hpp` and
//! `multi_file_list.cpp`.
//!
//! A lazily expanded `MultiFileList` turns globs into files as it is scanned,
//! and until it has seen the last glob it can say how many files it has *so
//! far* without being able to say that is all of them. The rules that read
//! those numbers - which of "no files", "one file" and "many files" a count
//! means, whether a count is exact, and whether a scan may reach past what is
//! already expanded - are here.
//!
//! As in [`crate::multi_file::data`], the storage stays in C++:
//! `multi_file_list.hpp` is a public header, and only the sources in
//! `src/common/multi_file` may include the crubit bindings.
//! `SimpleMultiFileList` keeps its `vector<OpenFileInfo>`, `LazyMultiFileList`
//! keeps its expanded files and its `all_files_expanded` flag, and both
//! read them through the rules below. Resolving a glob is C++ as well: it goes
//! through `FileSystem`, and it is the one part of the module that does I/O.

// The C++ spellings are kept, so `MakeFETCH_IF_AVAILABLE()` reads the same on both sides.
#![allow(nonstandard_style)]

use crate::multi_file::data::idx_t;

/// How many files a list resolved to. `NO_FILES` is what decides whether a
/// reader complains about an empty glob or reads nothing.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum FileExpandResult {
    NO_FILES,
    SINGLE_FILE,
    MULTIPLE_FILES,
}

/// Whether a file count is exact, or a lower bound a list that has not expanded
/// all of its globs can give.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum FileExpansionType {
    ALL_FILES_EXPANDED,
    NOT_ALL_FILES_KNOWN,
}

/// Whether a scan may reach past what is already expanded.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MultiFileListScanType {
    ALWAYS_FETCH,
    FETCH_IF_AVAILABLE,
}

/// How many files a list holds, and whether that count is exact.
///
/// The two travel together: a caller that needs an exact count checks the type,
/// and only one that does not - the cardinality estimate a planner is shown -
/// reads the count on its own.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct MultiFileCount {
    count: idx_t,
    expansion: FileExpansionType,
}

impl MultiFileCount {
    pub fn new(count: idx_t, expansion: FileExpansionType) -> Self { Self { count, expansion } }

    pub fn get_count(&self) -> idx_t { self.count }

    pub fn get_type(&self) -> FileExpansionType { self.expansion }
}

/// The count of files a list has expanded, read as a [`FileExpandResult`].
///
/// Both file lists answer `GetExpandResult` this way: `SimpleMultiFileList`
/// from the number of paths it was built with, `LazyMultiFileList` from the
/// number it has expanded so far.
pub fn expand_result(expanded_count: idx_t) -> FileExpandResult {
    if expanded_count > 1
    {
        FileExpandResult::MULTIPLE_FILES
    }
    else if expanded_count == 1
    {
        FileExpandResult::SINGLE_FILE
    }
    else
    {
        FileExpandResult::NO_FILES
    }
}

/// How many files a list holds, and whether that is exact.
///
/// `all_files_expanded` is `LazyMultiFileList::all_files_expanded`: a list that
/// has run out of globs has an exact count, one that has not may still grow.
/// `MultiFileList::GetFileCount` passes `true`, because a list that is not
/// lazily expanded has nothing left to expand.
pub fn file_count(count: idx_t, all_files_expanded: bool) -> MultiFileCount {
    let expansion = if all_files_expanded
    {
        FileExpansionType::ALL_FILES_EXPANDED
    }
    else
    {
        FileExpansionType::NOT_ALL_FILES_KNOWN
    };
    MultiFileCount::new(count, expansion)
}

/// Whether a scan reaches past what is already expanded, without asking.
///
/// `GlobMultiFileList` uses the distinction to expand a glob in whatever sized
/// batches the file system hands out: one file with `ALWAYS_FETCH`, and then
/// everything else the glob has already produced for free. The answer is what
/// lets `MultiFileList::Scan` skip `FileIsAvailable` for an `ALWAYS_FETCH`
/// scan, which would otherwise take the list's lock on every file.
pub fn scan_fetches_unconditionally(scan_type: MultiFileListScanType) -> bool {
    scan_type == MultiFileListScanType::ALWAYS_FETCH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_result_reads_the_count() {
        assert_eq!(expand_result(0), FileExpandResult::NO_FILES);
        assert_eq!(expand_result(1), FileExpandResult::SINGLE_FILE);
        assert_eq!(expand_result(2), FileExpandResult::MULTIPLE_FILES);
        assert_eq!(expand_result(1000), FileExpandResult::MULTIPLE_FILES);
    }

    #[test]
    fn a_file_count_is_exact_only_once_every_glob_is_used() {
        let count = file_count(3, false);
        assert_eq!(count.get_count(), 3);
        assert_eq!(count.get_type(), FileExpansionType::NOT_ALL_FILES_KNOWN);

        let count = file_count(3, true);
        assert_eq!(count.get_count(), 3);
        assert_eq!(count.get_type(), FileExpansionType::ALL_FILES_EXPANDED);
    }

    #[test]
    fn an_empty_list_is_a_file_count_of_zero() {
        // a list that has expanded nothing and is finished says so exactly
        let count = file_count(0, true);
        assert_eq!(count.get_count(), 0);
        assert_eq!(count.get_type(), FileExpansionType::ALL_FILES_EXPANDED);
        assert_eq!(expand_result(count.get_count()), FileExpandResult::NO_FILES);
    }

    #[test]
    fn an_always_fetch_scan_never_asks_whether_a_file_is_available() {
        // the scan type is what decides whether the check is made at all: an
        // ALWAYS_FETCH scan pays neither for FileIsAvailable nor for the list's
        // lock it takes
        assert!(scan_fetches_unconditionally(MultiFileListScanType::ALWAYS_FETCH));
        assert!(!scan_fetches_unconditionally(MultiFileListScanType::FETCH_IF_AVAILABLE));
    }

    #[test]
    fn every_variant_the_cpp_bridge_names_is_reachable() {
        // The bridge maps these to the C++ enums by variant name
        // (rust::FileExpandResult::MakeNO_FILES() and the rest), so every variant
        // it names has to be one of these - a variant the C++ names and no
        // kernel returns would be a conversion nothing ever hits, and adding a
        // variant on either side breaks the build until both are updated.
        assert_eq!(expand_result(0), FileExpandResult::NO_FILES);
        assert_eq!(expand_result(1), FileExpandResult::SINGLE_FILE);
        assert_eq!(expand_result(2), FileExpandResult::MULTIPLE_FILES);

        assert_eq!(file_count(0, true).get_type(), FileExpansionType::ALL_FILES_EXPANDED);
        assert_eq!(file_count(0, false).get_type(), FileExpansionType::NOT_ALL_FILES_KNOWN);
    }
}
