//! Foundation-style display projection of an IRI reference, never navigation.
//! Parse components without resolving a base or normalizing source dot segments.
use iri_string::types::IriReferenceStr;
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};

// Foundation accepts these characters after percent escaping. This repair is
// only attempted after strict parsing fails; valid IPv6 authority syntax stays
// untouched. Component boundaries and authority parsing remain library-owned.
const ESCAPES: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'\\')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}')
    .add(b'[')
    .add(b']');

pub(crate) fn preview_url(raw: &str) -> Option<String> {
    let escaped;
    let reference = match IriReferenceStr::new(raw) {
        Ok(reference) => reference,
        Err(_) => {
            let encoded = utf8_percent_encode(raw, ESCAPES).to_string();
            let mut repaired = String::with_capacity(encoded.len());
            for (index, character) in encoded.char_indices() {
                if character == '%'
                    && !encoded
                        .as_bytes()
                        .get(index + 1..index + 3)
                        .is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit))
                {
                    repaired.push_str("%25");
                } else {
                    repaired.push(character);
                }
            }
            escaped = repaired;
            IriReferenceStr::new(&escaped).ok()?
        }
    };
    let host = reference
        .authority_components()
        .map(|authority| {
            let decoded = percent_decode_str(authority.host())
                .decode_utf8()
                .unwrap_or_default();
            let host = decoded
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .unwrap_or(&decoded);
            if host.is_ascii() {
                // WHATWG Host::parse lowercases even ASCII reg-names; Foundation
                // retains their spelling. Only Unicode hostnames need IDNA mapping.
                host.to_owned()
            } else {
                url::Host::parse(host)
                    .map(|host| host.to_string())
                    .unwrap_or_default()
            }
        })
        .unwrap_or_default();
    let raw_path = reference.path_str();
    if reference.scheme_str().is_some()
        && reference.authority_str().is_none()
        && !raw_path.starts_with('/')
    {
        return Some(host); // Foundation has no filesystem path for opaque IRIs.
    }
    // URL.path uses Foundation's filesystem-path projection: remove one literal
    // directory delimiter, decode, then remove one encoded terminal delimiter.
    let path = if raw_path.len() > 1 {
        raw_path.strip_suffix('/').unwrap_or(raw_path)
    } else {
        raw_path
    };
    let mut decoded = percent_decode_str(path)
        .decode_utf8()
        .unwrap_or_default()
        .into_owned();
    if decoded.len() > 1
        && path
            .get(path.len().saturating_sub(3)..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case("%2f"))
    {
        decoded.pop();
    }
    Some(host + &decoded)
}
