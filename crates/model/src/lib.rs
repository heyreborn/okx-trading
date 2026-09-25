//! Stable cross-package contract primitives. Identity values are validated at
//! construction and deserialization boundaries. See the [package README](../README.md).

pub mod identity;
pub mod units;

/// Initial version of the shared model contract. This is a schema marker, not
/// evidence that any trading fact schema has been implemented.
pub const INITIAL_SCHEMA_VERSION: u16 = 1;

#[cfg(test)]
mod tests {
    use super::INITIAL_SCHEMA_VERSION;

    #[test]
    fn initial_schema_version_is_nonzero() {
        assert_ne!(INITIAL_SCHEMA_VERSION, 0);
    }
}
