//! What a person wrote, read as they wrote it.

/// `bytes` without the UTF-8 byte-order mark an editor may have put first.
/// Windows PowerShell 5.1 writes one with `-Encoding UTF8`, as do older
/// editors, and it is not part of the document: JSON's grammar leaves it
/// out, and a parser handed it reports the file as unreadable at line 1.
pub fn without_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// Parses a JSON document a person wrote: a manifest, a hook or server
/// declaration, a marketplace.
pub fn json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> serde_json::Result<T> {
    serde_json::from_slice(without_bom(bytes))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_document_reads_the_same_with_or_without_a_byte_order_mark() {
        let plain: serde_json::Value = super::json(br#"{"name":"guard"}"#).unwrap();
        let marked: serde_json::Value = super::json(b"\xEF\xBB\xBF{\"name\":\"guard\"}").unwrap();
        assert_eq!(plain, marked);
        assert!(super::json::<serde_json::Value>(b"\xEF\xBB\xBF").is_err());
    }
}
