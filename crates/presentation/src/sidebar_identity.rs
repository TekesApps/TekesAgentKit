//! Durable ClientKit thread identity ordering shared by Sidebar projections.
use std::cmp::Ordering;
use tekes_agent_contract::SessionAddress;

/// Compare Sidebar addresses by the durable thread identity used by ClientKit.
///
/// The storage key stays private so callers cannot accidentally use it as a routing ID.
pub fn compare_sidebar_identity(left: &SessionAddress, right: &SessionAddress) -> Ordering {
    storage_identity_key(left).cmp(&storage_identity_key(right))
}

fn storage_identity_key(address: &SessionAddress) -> String {
    if address.endpoint_id == "legacy-tekes-appserver" {
        return address.session_id.clone();
    }
    format!(
        "{}:{}7:session{}",
        address.endpoint_id.len(),
        address.endpoint_id,
        address.session_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(endpoint: &str, session: &str) -> SessionAddress {
        SessionAddress {
            endpoint_id: endpoint.into(),
            session_id: session.into(),
        }
    }

    #[test]
    fn compares_clientkit_durable_ids_across_endpoint_lengths_and_unicode_bytes() {
        let one_char = address("z", "same");
        let two_chars = address("ab", "same");
        // Tuple ordering puts "ab" first; the durable thread IDs put the shorter
        // endpoint key first because its UTF-8 length prefix is smaller.
        assert!(two_chars < one_char);
        assert_eq!(
            compare_sidebar_identity(&one_char, &two_chars),
            Ordering::Less
        );

        let unicode = address("中", "same");
        assert_eq!("中".len(), 3);
        assert_eq!(
            compare_sidebar_identity(&two_chars, &unicode),
            Ordering::Less
        );
    }

    #[test]
    fn preserves_legacy_native_identity_and_unambiguous_special_characters() {
        let native = address("legacy-tekes-appserver", "native:/session");
        let remote = address("legacy-tekes-appserver-extra", "native:/session");
        let embedded_colon = address("a:b", "c/d");
        let other = address("a", "b7:sessionc/d");

        assert_eq!(storage_identity_key(&native), "native:/session");
        assert_ne!(
            storage_identity_key(&embedded_colon),
            storage_identity_key(&other)
        );
        assert_ne!(compare_sidebar_identity(&native, &remote), Ordering::Equal);
    }
}
