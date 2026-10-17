use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_contract::*;
fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|w| w == needle)
}
pub(crate) fn decode(data: &[u8], content_type: &str) -> Result<Value> {
    if !content_type
        .to_ascii_lowercase()
        .starts_with("multipart/form-data;")
    {
        return Ok(serde_json::from_slice(data)?);
    }
    let boundary = content_type
        .split(';')
        .skip(1)
        .map(str::trim)
        .find_map(|s| s.strip_prefix("boundary="))
        .ok_or_else(|| Error::Protocol("missing multipart boundary".into()))?
        .trim_matches('"');
    require(
        !boundary.is_empty() && boundary.len() <= 200 && !boundary.contains(['\r', '\n']),
        "invalid multipart boundary",
    )?;
    let delimiter = format!("--{boundary}").into_bytes();
    let next = format!("\r\n--{boundary}").into_bytes();
    let mut remaining = data;
    let mut parts = BTreeMap::new();
    loop {
        require(
            remaining.starts_with(&delimiter),
            "invalid multipart delimiter",
        )?;
        remaining = &remaining[delimiter.len()..];
        if remaining.starts_with(b"--") {
            require(
                matches!(&remaining[2..], b"" | b"\r\n"),
                "invalid multipart end",
            )?;
            break;
        }
        require(
            remaining.starts_with(b"\r\n") && parts.len() < 16,
            "invalid multipart part",
        )?;
        remaining = &remaining[2..];
        let header_end = find(remaining, b"\r\n\r\n")
            .ok_or_else(|| Error::Protocol("missing multipart headers".into()))?;
        require(header_end <= 8192, "multipart headers too large")?;
        let headers = std::str::from_utf8(&remaining[..header_end])
            .map_err(|_| Error::Protocol("invalid multipart header".into()))?;
        let dispositions: Vec<_> = headers
            .split("\r\n")
            .filter(|line| {
                line.to_ascii_lowercase()
                    .starts_with("content-disposition:")
            })
            .collect();
        require(dispositions.len() == 1, "invalid part disposition")?;
        let name = dispositions[0]
            .split(';')
            .map(str::trim)
            .find_map(|s| s.strip_prefix("name=\"").and_then(|s| s.strip_suffix('"')))
            .ok_or_else(|| Error::Protocol("missing multipart name".into()))?;
        remaining = &remaining[header_end + 4..];
        let mut start = 0;
        let end = loop {
            let offset = find(&remaining[start..], &next)
                .ok_or_else(|| Error::Protocol("unterminated multipart".into()))?
                + start;
            let suffix = &remaining[offset + next.len()..];
            if suffix.starts_with(b"\r\n") || suffix.starts_with(b"--") {
                break offset;
            }
            start = offset + next.len();
        };
        require(
            parts
                .insert(name.to_owned(), remaining[..end].to_vec())
                .is_none(),
            "duplicate multipart part",
        )?;
        remaining = &remaining[end + 2..];
    }
    let metadata = parts
        .remove("metadata")
        .ok_or_else(|| Error::Protocol("missing multipart metadata".into()))?;
    let mut envelope: Value = serde_json::from_slice(&metadata)?;
    let attachments = envelope
        .as_object_mut()
        .and_then(|o| o.remove("attachments"))
        .ok_or_else(|| Error::Protocol("missing attachments".into()))?;
    let mut consumed = BTreeSet::new();
    let mut paths = BTreeSet::new();
    require(envelope["result"]["ok"] == true, "invalid binary result")?;
    for attachment in attachments
        .as_array()
        .ok_or_else(|| Error::Protocol("invalid attachments".into()))?
    {
        require(attachment["codec"] == "bytes", "unknown attachment codec")?;
        let part = string(attachment, "part")?;
        let path = array(attachment, "path")?;
        require(
            !path.is_empty()
                && consumed.insert(part.to_owned())
                && paths.insert(serde_json::to_string(path)?),
            "duplicate attachment path",
        )?;
        let mut target = &mut envelope["result"]["value"];
        for key in path {
            let key = key
                .as_str()
                .ok_or_else(|| Error::Protocol("invalid attachment path".into()))?;
            target = target
                .as_object_mut()
                .and_then(|o| o.get_mut(key))
                .ok_or_else(|| Error::Protocol("missing attachment slot".into()))?;
        }
        require(target.is_null(), "attachment slot must be null")?;
        let bytes = parts
            .get(part)
            .ok_or_else(|| Error::Protocol("missing binary part".into()))?;
        *target = Value::String(STANDARD.encode(bytes));
    }
    require(consumed.len() == parts.len(), "unreferenced binary part")?;
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_parts_preserve_false_delimiters_and_require_unique_slots() {
        let metadata = serde_json::json!({"result":{"ok":true,"value":{"data":null}},"attachments":[{"codec":"bytes","part":"body","path":["data"]}]});
        let mut bytes=format!("--b\r\nContent-Disposition: form-data; name=\"metadata\"\r\n\r\n{metadata}\r\n--b\r\nContent-Disposition: form-data; name=\"body\"\r\n\r\n").into_bytes();
        let payload = b"\0\xff\r\n--b-not-a-delimiter";
        bytes.extend(payload);
        bytes.extend(b"\r\n--b--\r\n");
        let result = decode(&bytes, "multipart/form-data; boundary=b").unwrap();
        assert_eq!(
            STANDARD
                .decode(result["result"]["value"]["data"].as_str().unwrap())
                .unwrap(),
            payload
        );
        assert!(decode(&bytes[..bytes.len() - 5], "multipart/form-data; boundary=b").is_err());
        assert!(decode(&bytes, "multipart/form-data; boundary=other").is_err());
    }
}
