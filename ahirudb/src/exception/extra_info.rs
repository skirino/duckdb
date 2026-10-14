//! The `extra_info` keys `Exception::SetQueryLocation` and
//! `Exception::InitializeExtraInfo` set, ported from
//! `src/common/exception.cpp`.

use crate::exception::types::{
    ExtraInfo,
    QueryLocationLayout,
};

/// The entries `Exception::SetQueryLocation` adds for `location`: nothing for a
/// location that is not known, otherwise the start offset and the `[start,
/// length]` range.
///
/// `position` is kept for the consumers that only ever read the start offset;
/// `location` is the structured form.
pub(crate) fn set_query_location(location: QueryLocationLayout) -> ExtraInfo {
    if !location.is_valid()
    {
        return Vec::new();
    }
    let offset = location.offset.to_string();
    let range = format!("[{},{}]", location.offset, location.length);
    vec![(b"position".to_vec(), offset.into_bytes()), (b"location".to_vec(), range.into_bytes())]
}

/// The entries `Exception::InitializeExtraInfo(subtype, location)` starts from:
/// the subtype, then the location.
pub(crate) fn initialize_extra_info(subtype: &[u8], location: QueryLocationLayout) -> ExtraInfo {
    let mut extra_info = vec![(b"error_subtype".to_vec(), subtype.to_vec())];
    extra_info.extend(set_query_location(location));
    extra_info
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exception::types::QueryLocationLayout;

    fn keys(extra_info: &ExtraInfo) -> Vec<String> {
        extra_info
            .iter()
            .map(|(key, _)| String::from_utf8(key.clone()).unwrap())
            .collect()
    }

    fn values(extra_info: &ExtraInfo) -> Vec<String> {
        extra_info
            .iter()
            .map(|(_, value)| String::from_utf8(value.clone()).unwrap())
            .collect()
    }

    #[test]
    fn an_unknown_location_adds_nothing() {
        assert!(set_query_location(QueryLocationLayout::invalid()).is_empty());
    }

    #[test]
    fn a_known_location_adds_the_start_offset_and_the_range() {
        let extra_info = set_query_location(QueryLocationLayout::new(42, 7));
        assert_eq!(vec!["position", "location"], keys(&extra_info));
        assert_eq!(vec!["42", "[42,7]"], values(&extra_info));
    }

    #[test]
    fn a_point_location_has_a_zero_length_range() {
        let extra_info = set_query_location(QueryLocationLayout::new(0, 0));
        assert_eq!(vec!["0", "[0,0]"], values(&extra_info));
    }

    #[test]
    fn the_subtype_comes_before_the_location() {
        let extra_info = initialize_extra_info(b"COLUMN_NOT_FOUND", QueryLocationLayout::new(3, 1));
        assert_eq!(vec!["error_subtype", "position", "location"], keys(&extra_info));
        assert_eq!(vec!["COLUMN_NOT_FOUND", "3", "[3,1]"], values(&extra_info));
    }

    #[test]
    fn a_subtype_without_a_location_is_just_the_subtype() {
        let extra_info = initialize_extra_info(b"MISSING_ENTRY", QueryLocationLayout::invalid());
        assert_eq!(vec!["error_subtype"], keys(&extra_info));
    }
}
