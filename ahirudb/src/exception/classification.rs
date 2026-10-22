//! Exception classification helpers, ported from the `switch` statements of
//! `src/common/exception.cpp`.

use crate::exception::types::ExceptionType;

/// Whether this exception type invalidates the current transaction.
/// Matching `Exception::InvalidatesTransaction`.
pub(crate) fn invalidates_transaction(exception_type: ExceptionType) -> bool {
    !matches!(
        exception_type,
        ExceptionType::BINDER
            | ExceptionType::CATALOG
            | ExceptionType::CONNECTION
            | ExceptionType::PARAMETER_NOT_ALLOWED
            | ExceptionType::PARSER
            | ExceptionType::PERMISSION
            | ExceptionType::RESOURCE_IN_USE
    )
}

/// Whether this exception type invalidates the entire database.
/// Matching `Exception::InvalidatesDatabase`.
pub(crate) fn invalidates_database(exception_type: ExceptionType) -> bool {
    matches!(
        exception_type,
        ExceptionType::DATA_CORRUPTION | ExceptionType::FATAL
    )
}

/// Whether this exception type can occur during query execution.
/// Matching `Exception::IsExecutionError`.
pub(crate) fn is_execution_error(exception_type: ExceptionType) -> bool {
    matches!(
        exception_type,
        ExceptionType::INVALID_INPUT | ExceptionType::OUT_OF_RANGE | ExceptionType::CONVERSION
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every type, so that adding a variant cannot leave a case unclassified.
    fn every_type() -> impl Iterator<Item = ExceptionType> {
        (0..ExceptionType::COUNT as u8).map(|i| ExceptionType::from_u8(i).unwrap())
    }

    #[test]
    fn the_transaction_exemptions() {
        for exception_type in [
            ExceptionType::BINDER,
            ExceptionType::CATALOG,
            ExceptionType::CONNECTION,
            ExceptionType::PARAMETER_NOT_ALLOWED,
            ExceptionType::PARSER,
            ExceptionType::PERMISSION,
            ExceptionType::RESOURCE_IN_USE,
        ] {
            assert!(
                !invalidates_transaction(exception_type),
                "{exception_type:?} should not invalidate the transaction"
            );
        }
        assert!(invalidates_transaction(ExceptionType::INTERNAL));
        assert!(invalidates_transaction(ExceptionType::OUT_OF_RANGE));
        assert!(invalidates_transaction(ExceptionType::CONVERSION));
    }

    #[test]
    fn only_corruption_and_fatal_invalidate_the_database() {
        let invalidating: Vec<ExceptionType> =
            every_type().filter(|t| invalidates_database(*t)).collect();
        assert_eq!(
            vec![ExceptionType::FATAL, ExceptionType::DATA_CORRUPTION],
            invalidating
        );
    }

    #[test]
    fn only_conversion_errors_are_execution_errors() {
        let execution_errors: Vec<ExceptionType> =
            every_type().filter(|t| is_execution_error(*t)).collect();
        assert_eq!(
            vec![
                ExceptionType::OUT_OF_RANGE,
                ExceptionType::CONVERSION,
                ExceptionType::INVALID_INPUT
            ],
            execution_errors
        );
    }

    #[test]
    fn every_type_is_classified() {
        let total = every_type().count();
        assert_eq!(ExceptionType::COUNT, total);
        let exempt = every_type()
            .filter(|t| !invalidates_transaction(*t))
            .count();
        assert_eq!(7, exempt);
    }
}
