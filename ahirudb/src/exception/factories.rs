//! The message and `extra_info` that the exception factories of
//! `src/common/exception` build, ported from those files.
//!
//! What is *not* here is the substitution of the `%s` placeholders. The C++
//! originals go through `StringUtil::Format`, which is
//! `Exception::ConstructMessage`, so once the pieces are in every message below
//! is plain concatenation. The pieces only C++ can compute - candidate lists
//! ranked by `StringUtil::TopNLevenshtein`, and `Function::CallToString`'s
//! catalog/schema prefix rule - are passed in already rendered rather than
//! reimplemented here.

use crate::exception::{
    extra_info::initialize_extra_info,
    types::{
        ExceptionPayload,
        QueryLocationLayout,
        into_payload,
    },
};

/// Joins with `separator`, which is what `StringUtil::Join` does for a list of
/// strings.
fn join(values: &[&[u8]], separator: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    for (index, value) in values.iter().enumerate()
    {
        if index > 0
        {
            result.extend_from_slice(separator);
        }
        result.extend_from_slice(value);
    }
    result
}

/// `BinderException::ColumnNotFound`.
///
/// `candidate_message` is `StringUtil::CandidatesMessage` over
/// `similar_bindings`, which C++ renders because picking candidates is its job.
pub(crate) fn binder_column_not_found(
    name: &[u8],
    similar_bindings: &[&[u8]],
    candidate_message: &[u8],
    query_location: QueryLocationLayout,
) -> ExceptionPayload {
    let mut message = b"Referenced column \"".to_vec();
    message.extend_from_slice(name);
    message.push(b'"');

    let mut extra_info = initialize_extra_info(b"COLUMN_NOT_FOUND", query_location);
    extra_info.push((b"name".to_vec(), name.to_vec()));

    if !similar_bindings.is_empty()
    {
        message.extend_from_slice(b" not found in FROM clause!");
        message.extend_from_slice(candidate_message);
        extra_info.push((b"candidates".to_vec(), join(similar_bindings, b",")));
    }
    else
    {
        message.extend_from_slice(b" was not found because the FROM clause is missing");
    }

    into_payload(message, extra_info)
}

/// `BinderException::NoMatchingFunction`.
///
/// `call_string` is `Function::CallToString`, which C++ renders because whether
/// the `catalog.schema.` prefix belongs in front of the name follows
/// `Function`'s own rules.
pub(crate) fn binder_no_matching_function(
    catalog_name: &[u8],
    schema_name: &[u8],
    name: &[u8],
    call_string: &[u8],
    candidates: &[&[u8]],
) -> ExceptionPayload {
    let mut message = b"No function matches the given name and argument types '".to_vec();
    message.extend_from_slice(call_string);
    message.extend_from_slice(
        b"'. You might need to add explicit type casts.\n\tCandidate functions:\n",
    );
    for candidate in candidates
    {
        message.push(b'\t');
        message.extend_from_slice(candidate);
        message.push(b'\n');
    }

    let mut extra_info =
        initialize_extra_info(b"NO_MATCHING_FUNCTION", QueryLocationLayout::invalid());
    extra_info.push((b"name".to_vec(), name.to_vec()));
    if !catalog_name.is_empty()
    {
        extra_info.push((b"catalog".to_vec(), catalog_name.to_vec()));
    }
    if !schema_name.is_empty()
    {
        extra_info.push((b"schema".to_vec(), schema_name.to_vec()));
    }
    extra_info.push((b"call".to_vec(), call_string.to_vec()));
    if !candidates.is_empty()
    {
        extra_info.push((b"candidates".to_vec(), join(candidates, b",")));
    }

    into_payload(message, extra_info)
}

/// `CatalogException::MissingEntry(const EntryLookupInfo &, const string
/// &suggestion)`.
///
/// `version_info` is the ` at <unit> <value>` clause of an `AT` lookup, which
/// C++ renders because it comes from a `BoundAtClause`.
pub(crate) fn catalog_missing_entry(
    catalog_type: &[u8],
    name: &[u8],
    suggestion: &[u8],
    version_info: &[u8],
    query_location: QueryLocationLayout,
) -> ExceptionPayload {
    let mut message = catalog_type.to_vec();
    message.extend_from_slice(b" with name ");
    message.extend_from_slice(name);
    message.extend_from_slice(b" does not exist");
    message.extend_from_slice(version_info);
    message.push(b'!');

    let mut extra_info = initialize_extra_info(b"MISSING_ENTRY", query_location);
    extra_info.push((b"name".to_vec(), name.to_vec()));
    extra_info.push((b"type".to_vec(), catalog_type.to_vec()));

    if !suggestion.is_empty()
    {
        extra_info.push((b"candidates".to_vec(), suggestion.to_vec()));
        message.extend_from_slice(b"\nDid you mean \"");
        message.extend_from_slice(suggestion);
        message.extend_from_slice(b"\"?");
    }

    into_payload(message, extra_info)
}

/// `CatalogException::MissingEntry(const string &type, const Identifier &name,
/// ...)`.
///
/// `candidate_message` is `StringUtil::CandidatesErrorMessage`, which C++
/// renders because it ranks the suggestions by Levenshtein distance first.
pub(crate) fn catalog_missing_entry_suggestions(
    catalog_type: &[u8],
    name: &[u8],
    suggestions: &[&[u8]],
    candidate_message: &[u8],
    query_location: QueryLocationLayout,
) -> ExceptionPayload {
    let mut message = b"unrecognized ".to_vec();
    message.extend_from_slice(catalog_type);
    message.push(b' ');
    message.extend_from_slice(name);
    message.push(b'\n');
    message.extend_from_slice(candidate_message);

    // `error_subtype` is already what `InitializeExtraInfo` starts from, which
    // is why the C++ original setting it a second time changed nothing.
    let mut extra_info = initialize_extra_info(b"MISSING_ENTRY", query_location);
    extra_info.push((b"name".to_vec(), name.to_vec()));
    extra_info.push((b"type".to_vec(), catalog_type.to_vec()));
    if !suggestions.is_empty()
    {
        extra_info.push((b"candidates".to_vec(), join(suggestions, b", ")));
    }

    into_payload(message, extra_info)
}

/// `CatalogException::EntryAlreadyExists`.
pub(crate) fn catalog_entry_already_exists(catalog_type: &[u8], name: &[u8]) -> ExceptionPayload {
    let mut message = catalog_type.to_vec();
    message.extend_from_slice(b" with name \"");
    message.extend_from_slice(name);
    message.extend_from_slice(b"\" already exists!");

    // The C++ original hands this a default-constructed `QueryLocation`, so a
    // duplicate entry never carries `position`/`location`, however well
    // known the statement is.
    let mut extra_info =
        initialize_extra_info(b"ENTRY_ALREADY_EXISTS", QueryLocationLayout::invalid());
    extra_info.push((b"name".to_vec(), name.to_vec()));
    extra_info.push((b"type".to_vec(), catalog_type.to_vec()));

    into_payload(message, extra_info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exception::types::{
        free_payload,
        live_payloads,
    };

    /// Renders a payload the way the C++ side does, then frees it, so that a
    /// leak or a double free in the marshalling shows up here rather than
    /// in a query.
    fn render(payload: ExceptionPayload) -> (String, String) {
        let message = payload.message.as_slice().to_vec();
        let entries = if payload.entry_count == 0
        {
            &[][..]
        }
        else
        {
            // SAFETY: the payload owns `entry_count` entries at `entries`, and
            // `message` above only borrowed the message allocation.
            unsafe { std::slice::from_raw_parts(payload.entries, payload.entry_count) }
        };
        let extra_info = entries
            .iter()
            .map(|entry| {
                let key = String::from_utf8(entry.key.as_slice().to_vec()).unwrap();
                let value = String::from_utf8(entry.value.as_slice().to_vec()).unwrap();
                format!("{key}={value}")
            })
            .collect::<Vec<_>>()
            .join(",");

        free_payload(payload);
        (String::from_utf8(message).unwrap(), extra_info)
    }

    #[test]
    fn column_not_found_lists_the_candidate_bindings() {
        let (message, extra_info) = render(binder_column_not_found(
            b"i",
            &[b"id".as_slice(), b"idx".as_slice()],
            b"\nCandidate bindings: \"id\", \"idx\"",
            QueryLocationLayout::new(10, 2),
        ));
        assert_eq!(
            "Referenced column \"i\" not found in FROM clause!\nCandidate bindings: \"id\", \
             \"idx\"",
            message
        );
        assert_eq!(
            "error_subtype=COLUMN_NOT_FOUND,position=10,location=[10,2],name=i,candidates=id,idx",
            extra_info
        );
    }

    #[test]
    fn column_not_found_without_candidates_mentions_the_missing_from_clause() {
        let (message, extra_info) =
            render(binder_column_not_found(b"i", &[], b"", QueryLocationLayout::invalid()));
        assert_eq!(
            "Referenced column \"i\" was not found because the FROM clause is missing",
            message
        );
        assert_eq!("error_subtype=COLUMN_NOT_FOUND,name=i", extra_info);
    }

    #[test]
    fn no_matching_function_renders_the_call_cpp_passed_in() {
        // The catalog and schema are the ones the candidate functions live in,
        // so for a built-in they are `system`/`main` - which
        // `Function::CallToString` leaves out of the call string but
        // which still land in the extra_info.
        let (message, extra_info) = render(binder_no_matching_function(
            b"system",
            b"main",
            b"struct_values",
            b"struct_values(INTEGER_LITERAL)",
            &[],
        ));
        assert_eq!(
            "No function matches the given name and argument types \
             'struct_values(INTEGER_LITERAL)'. You might need to add explicit type \
             casts.\n\tCandidate functions:\n",
            message
        );
        assert_eq!(
            "error_subtype=NO_MATCHING_FUNCTION,name=struct_values,catalog=system,schema=main,\
             call=struct_values(INTEGER_LITERAL)",
            extra_info
        );
    }

    #[test]
    fn no_matching_function_leaves_out_empty_catalog_and_schema() {
        let (_, extra_info) = render(binder_no_matching_function(b"", b"", b"f", b"f()", &[]));
        assert_eq!("error_subtype=NO_MATCHING_FUNCTION,name=f,call=f()", extra_info);
    }

    #[test]
    fn no_matching_function_names_the_candidates() {
        let (message, extra_info) = render(binder_no_matching_function(
            b"my_catalog",
            b"my_schema",
            b"f",
            b"my_catalog.my_schema.f(INTEGER)",
            &[b"f(INTEGER)".as_slice(), b"f(VARCHAR)".as_slice()],
        ));
        assert_eq!(
            "No function matches the given name and argument types \
             'my_catalog.my_schema.f(INTEGER)'. You might need to add explicit type \
             casts.\n\tCandidate functions:\n\tf(INTEGER)\n\tf(VARCHAR)\n",
            message
        );
        assert_eq!(
            "error_subtype=NO_MATCHING_FUNCTION,name=f,catalog=my_catalog,schema=my_schema,\
             call=my_catalog.my_schema.f(INTEGER),candidates=f(INTEGER),f(VARCHAR)",
            extra_info
        );
    }

    #[test]
    fn missing_entry_suggests_a_name() {
        let (message, extra_info) = render(catalog_missing_entry(
            b"Table",
            b"tbl",
            b"tblx",
            b" at 2020-01-01",
            QueryLocationLayout::new(5, 4),
        ));
        assert_eq!(
            "Table with name tbl does not exist at 2020-01-01!\nDid you mean \"tblx\"?",
            message
        );
        assert_eq!(
            "error_subtype=MISSING_ENTRY,position=5,location=[5,4],name=tbl,type=Table,\
             candidates=tblx",
            extra_info
        );
    }

    #[test]
    fn missing_entry_without_a_suggestion_has_no_did_you_mean() {
        let (message, extra_info) = render(catalog_missing_entry(
            b"Table",
            b"tbl",
            b"",
            b"",
            QueryLocationLayout::invalid(),
        ));
        assert_eq!("Table with name tbl does not exist!", message);
        assert_eq!("error_subtype=MISSING_ENTRY,name=tbl,type=Table", extra_info);
    }

    #[test]
    fn missing_entry_with_suggestions_uses_the_ranked_candidate_message() {
        let (message, extra_info) = render(catalog_missing_entry_suggestions(
            b"schema",
            b"my_schema",
            &[b"my_schem".as_slice(), b"other".as_slice()],
            b"\nDid you mean: \"my_schem\"",
            QueryLocationLayout::invalid(),
        ));
        assert_eq!("unrecognized schema my_schema\n\nDid you mean: \"my_schem\"", message);
        assert_eq!(
            "error_subtype=MISSING_ENTRY,name=my_schema,type=schema,candidates=my_schem, other",
            extra_info
        );
    }

    #[test]
    fn entry_already_exists_carries_no_query_location() {
        let (message, extra_info) = render(catalog_entry_already_exists(b"Table", b"tbl"));
        assert_eq!("Table with name \"tbl\" already exists!", message);
        assert_eq!("error_subtype=ENTRY_ALREADY_EXISTS,name=tbl,type=Table", extra_info);
    }

    #[test]
    fn names_are_carried_through_as_bytes() {
        // An identifier is not required to be UTF-8, and the message has to
        // reproduce the bytes rather than replace them.
        let payload = catalog_entry_already_exists(b"Table", b"\xff\xfe");
        assert_eq!(
            b"Table with name \"\xff\xfe\" already exists!".to_vec(),
            payload.message.as_slice()
        );
        assert_eq!(3, payload.entry_count);
        free_payload(payload);
    }

    #[test]
    fn payloads_are_released_without_leaking() {
        // One round per factory, so that a factory whose payload is not taken
        // back shows up as a count that keeps climbing.
        let before = live_payloads();
        for _ in 0..100
        {
            let name = catalog_entry_already_exists(b"Table", b"tbl");
            let location = binder_column_not_found(
                b"i",
                &[b"id".as_slice()],
                b"\nCandidate bindings: \"id\"",
                QueryLocationLayout::new(1, 1),
            );
            let no_location =
                binder_column_not_found(b"i", &[], b"", QueryLocationLayout::invalid());
            let function =
                binder_no_matching_function(b"system", b"main", b"f", b"f()", &[b"f()".as_slice()]);
            let entries = catalog_missing_entry(
                b"Table",
                b"tbl",
                b"tblx",
                b"",
                QueryLocationLayout::new(2, 2),
            );
            let suggestions = catalog_missing_entry_suggestions(
                b"schema",
                b"s",
                &[b"t".as_slice()],
                b"\nDid you mean: \"t\"",
                QueryLocationLayout::invalid(),
            );
            let only_location =
                crate::exception::extra_info::set_query_location(QueryLocationLayout::new(3, 3));
            let only_location = into_payload(Vec::new(), only_location);
            for payload in
                [name, location, no_location, function, entries, suggestions, only_location]
            {
                free_payload(payload);
            }
        }
        assert_eq!(before, live_payloads(), "a payload was not handed back");
    }
}
