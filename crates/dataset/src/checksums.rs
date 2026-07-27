//! Deterministic checksums and identifiers, mirroring `quality::logical_checksum`.

use serde::Serialize;
use sha2::{Digest, Sha256};

#[must_use]
pub fn logical_checksum<T: Serialize>(value: &T) -> String {
    let serialized =
        serde_json::to_vec(value).unwrap_or_else(|error| error.to_string().into_bytes());
    format!("{:x}", Sha256::digest(serialized))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_is_deterministic_and_order_independent_of_call_site() {
        let a = logical_checksum(&("x", 1, true));
        let b = logical_checksum(&("x", 1, true));
        assert_eq!(a, b);
    }

    #[test]
    fn checksum_differs_when_content_differs() {
        assert_ne!(logical_checksum(&1), logical_checksum(&2));
    }
}
