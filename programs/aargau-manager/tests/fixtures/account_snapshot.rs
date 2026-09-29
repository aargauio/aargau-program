//! Committed mainnet account snapshots, one JSON object per account:
//!
//! ```json
//! { "slot": 1, "pubkey": "…", "owner": "…", "lamports": 1,
//!   "executable": false, "data_base64": "…" }
//! ```
//!
//! The files are written by `dump-raydium-fixtures.sh` straight from an RPC
//! `getMultipleAccounts` response. The parser accepts exactly that flat shape
//! (string, unsigned integer and boolean values, no escapes) so the test
//! crate needs no JSON or base64 dependency.

use std::path::Path;
use std::str::FromStr;

use anchor_lang::prelude::Pubkey;

/// One account as it existed on mainnet at `slot`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub slot: u64,
    pub pubkey: Pubkey,
    pub owner: Pubkey,
    pub lamports: u64,
    pub is_executable: bool,
    pub data: Vec<u8>,
}

/// Read and parse one snapshot file.
pub fn read_account_snapshot(path: &Path) -> Result<AccountSnapshot, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    parse_account_snapshot(&json).map_err(|error| format!("{}: {error}", path.display()))
}

/// Parse one snapshot object. Every field is required; unknown fields are
/// rejected so a changed dump format fails loudly.
pub fn parse_account_snapshot(json: &str) -> Result<AccountSnapshot, String> {
    let mut fields = SnapshotFields::default();
    for (name, value) in parse_flat_json_object(json)? {
        let is_duplicate = match name.as_str() {
            "slot" => fields.slot.replace(value.into_u64(&name)?).is_some(),
            "pubkey" => fields.pubkey.replace(value.into_pubkey(&name)?).is_some(),
            "owner" => fields.owner.replace(value.into_pubkey(&name)?).is_some(),
            "lamports" => fields.lamports.replace(value.into_u64(&name)?).is_some(),
            "executable" => fields
                .is_executable
                .replace(value.into_bool(&name)?)
                .is_some(),
            "data_base64" => fields
                .data
                .replace(decode_base64(&value.into_string(&name)?)?)
                .is_some(),
            _ => return Err(format!("unknown snapshot field `{name}`")),
        };
        if is_duplicate {
            return Err(format!("duplicate snapshot field `{name}`"));
        }
    }
    fields.into_snapshot()
}

#[derive(Default)]
struct SnapshotFields {
    slot: Option<u64>,
    pubkey: Option<Pubkey>,
    owner: Option<Pubkey>,
    lamports: Option<u64>,
    is_executable: Option<bool>,
    data: Option<Vec<u8>>,
}

impl SnapshotFields {
    fn into_snapshot(self) -> Result<AccountSnapshot, String> {
        let missing = |name: &str| format!("missing snapshot field `{name}`");
        Ok(AccountSnapshot {
            slot: self.slot.ok_or_else(|| missing("slot"))?,
            pubkey: self.pubkey.ok_or_else(|| missing("pubkey"))?,
            owner: self.owner.ok_or_else(|| missing("owner"))?,
            lamports: self.lamports.ok_or_else(|| missing("lamports"))?,
            is_executable: self.is_executable.ok_or_else(|| missing("executable"))?,
            data: self.data.ok_or_else(|| missing("data_base64"))?,
        })
    }
}

/// A scalar JSON value of the flat snapshot shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonScalar {
    String(String),
    UnsignedInteger(u64),
    Bool(bool),
}

impl JsonScalar {
    fn into_string(self, field: &str) -> Result<String, String> {
        match self {
            Self::String(value) => Ok(value),
            other => Err(format!("field `{field}`: expected a string, got {other:?}")),
        }
    }

    fn into_u64(self, field: &str) -> Result<u64, String> {
        match self {
            Self::UnsignedInteger(value) => Ok(value),
            other => Err(format!(
                "field `{field}`: expected an integer, got {other:?}"
            )),
        }
    }

    fn into_bool(self, field: &str) -> Result<bool, String> {
        match self {
            Self::Bool(value) => Ok(value),
            other => Err(format!(
                "field `{field}`: expected a boolean, got {other:?}"
            )),
        }
    }

    fn into_pubkey(self, field: &str) -> Result<Pubkey, String> {
        let text = self.into_string(field)?;
        Pubkey::from_str(&text).map_err(|error| format!("field `{field}`: {error}"))
    }
}

/// Parse `{ "name": scalar, … }` into its fields, in source order.
pub fn parse_flat_json_object(json: &str) -> Result<Vec<(String, JsonScalar)>, String> {
    let mut cursor = JsonCursor::new(json);
    let mut fields = Vec::new();
    cursor.expect(b'{')?;
    if !cursor.consume_if(b'}') {
        loop {
            let name = cursor.parse_string()?;
            cursor.expect(b':')?;
            fields.push((name, cursor.parse_scalar()?));
            if cursor.consume_if(b'}') {
                break;
            }
            cursor.expect(b',')?;
        }
    }
    cursor.expect_end()?;
    Ok(fields)
}

struct JsonCursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> JsonCursor<'a> {
    fn new(json: &'a str) -> Self {
        Self {
            bytes: json.as_bytes(),
            position: 0,
        }
    }

    fn skip_whitespace(&mut self) {
        while self
            .bytes
            .get(self.position)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            self.position += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_whitespace();
        self.bytes.get(self.position).copied()
    }

    fn consume_if(&mut self, expected: u8) -> bool {
        let is_match = self.peek() == Some(expected);
        if is_match {
            self.position += 1;
        }
        is_match
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        if self.consume_if(expected) {
            Ok(())
        } else {
            Err(format!(
                "expected `{}` at byte {}",
                char::from(expected),
                self.position
            ))
        }
    }

    fn expect_end(&mut self) -> Result<(), String> {
        match self.peek() {
            None => Ok(()),
            Some(_) => Err(format!("trailing content at byte {}", self.position)),
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let start = self.position;
        loop {
            match self.bytes.get(self.position) {
                None => return Err("unterminated string".to_string()),
                Some(b'\\') => return Err("escaped strings are not supported".to_string()),
                Some(b'"') => break,
                Some(_) => self.position += 1,
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.position])
            .map_err(|error| format!("string is not UTF-8: {error}"))?
            .to_string();
        self.position += 1;
        Ok(text)
    }

    fn parse_scalar(&mut self) -> Result<JsonScalar, String> {
        match self.peek() {
            Some(b'"') => self.parse_string().map(JsonScalar::String),
            Some(b't') => self.parse_keyword("true", JsonScalar::Bool(true)),
            Some(b'f') => self.parse_keyword("false", JsonScalar::Bool(false)),
            Some(byte) if byte.is_ascii_digit() => self.parse_unsigned_integer(),
            _ => Err(format!("unsupported value at byte {}", self.position)),
        }
    }

    fn parse_keyword(&mut self, keyword: &str, value: JsonScalar) -> Result<JsonScalar, String> {
        if self.bytes[self.position..].starts_with(keyword.as_bytes()) {
            self.position += keyword.len();
            Ok(value)
        } else {
            Err(format!("unsupported value at byte {}", self.position))
        }
    }

    fn parse_unsigned_integer(&mut self) -> Result<JsonScalar, String> {
        let mut value: u64 = 0;
        while let Some(byte) = self.bytes.get(self.position).filter(|b| b.is_ascii_digit()) {
            value = value
                .checked_mul(10)
                .and_then(|scaled| scaled.checked_add(u64::from(byte - b'0')))
                .ok_or_else(|| format!("integer overflows u64 at byte {}", self.position))?;
            self.position += 1;
        }
        if matches!(self.bytes.get(self.position), Some(b'.' | b'e' | b'E')) {
            return Err(format!("non-integer number at byte {}", self.position));
        }
        Ok(JsonScalar::UnsignedInteger(value))
    }
}

/// Decode standard (RFC 4648 §4) padded base64, as emitted by Solana RPC.
pub fn decode_base64(encoded: &str) -> Result<Vec<u8>, String> {
    let (quads, remainder) = encoded.as_bytes().as_chunks::<4>();
    if !remainder.is_empty() {
        return Err(format!(
            "base64 length {} is not a multiple of 4",
            encoded.len()
        ));
    }
    let mut decoded = Vec::with_capacity(quads.len() * 3);
    let quad_count = quads.len();
    for (quad_index, quad) in quads.iter().enumerate() {
        let padding = quad.iter().rev().take_while(|&&byte| byte == b'=').count();
        let is_last_quad = quad_index + 1 == quad_count;
        if padding > 2 || (padding > 0 && !is_last_quad) {
            return Err(format!("misplaced base64 padding in quad {quad_index}"));
        }
        let mut bits: u32 = 0;
        for &byte in &quad[..4 - padding] {
            bits = (bits << 6) | u32::from(base64_sextet(byte)?);
        }
        bits <<= 6 * padding;
        let [_, first, second, third] = bits.to_be_bytes();
        decoded.extend_from_slice(&[first, second, third][..3 - padding]);
    }
    Ok(decoded)
}

fn base64_sextet(byte: u8) -> Result<u8, String> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(format!("invalid base64 byte {byte:#04x}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAPSHOT_JSON: &str = r#"{
  "slot": 451691132,
  "pubkey": "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK",
  "owner": "11111111111111111111111111111111",
  "lamports": 18446744073709551615,
  "executable": true,
  "data_base64": "Zm9vYg=="
}"#;

    #[test]
    fn decodes_rfc4648_test_vectors() {
        for (encoded, expected) in [
            ("", ""),
            ("Zg==", "f"),
            ("Zm8=", "fo"),
            ("Zm9v", "foo"),
            ("Zm9vYg==", "foob"),
            ("Zm9vYmE=", "fooba"),
            ("Zm9vYmFy", "foobar"),
        ] {
            assert_eq!(decode_base64(encoded).unwrap(), expected.as_bytes());
        }
    }

    #[test]
    fn decodes_every_sextet_including_plus_and_slash() {
        assert_eq!(decode_base64("+/+/").unwrap(), [0xfb, 0xff, 0xbf]);
        assert_eq!(decode_base64("AAAA").unwrap(), [0, 0, 0]);
    }

    #[test]
    fn rejects_malformed_base64() {
        assert!(decode_base64("Zg=").is_err(), "length not a multiple of 4");
        assert!(decode_base64("Z===").is_err(), "three padding bytes");
        assert!(decode_base64("Zg==Zm9v").is_err(), "padding before the end");
        assert!(decode_base64("Zm9-").is_err(), "URL-safe alphabet");
    }

    #[test]
    fn parses_a_snapshot_with_u64_max_lamports() {
        let snapshot = parse_account_snapshot(SNAPSHOT_JSON).unwrap();
        assert_eq!(snapshot.slot, 451_691_132);
        assert_eq!(
            snapshot.pubkey,
            aargau_manager::constants::RAYDIUM_CLMM_PROGRAM_ID
        );
        assert_eq!(snapshot.owner, Pubkey::default());
        assert_eq!(snapshot.lamports, u64::MAX);
        assert!(snapshot.is_executable);
        assert_eq!(snapshot.data, b"foob");
    }

    #[test]
    fn rejects_missing_duplicate_and_unknown_fields() {
        let missing = SNAPSHOT_JSON.replace("\"executable\": true,", "");
        assert!(parse_account_snapshot(&missing)
            .unwrap_err()
            .contains("missing"));

        let duplicate = SNAPSHOT_JSON.replace("\"executable\": true,", "\"slot\": 1,");
        assert!(parse_account_snapshot(&duplicate)
            .unwrap_err()
            .contains("duplicate"));

        let unknown = SNAPSHOT_JSON.replace("\"executable\"", "\"rentEpoch\"");
        assert!(parse_account_snapshot(&unknown)
            .unwrap_err()
            .contains("unknown"));
    }

    #[test]
    fn rejects_values_outside_the_flat_shape() {
        for json in [
            r#"{"slot": 18446744073709551616}"#,
            r#"{"slot": 1.5}"#,
            r#"{"slot": -1}"#,
            r#"{"slot": null}"#,
            r#"{"pubkey": "a\"b"}"#,
            r#"{"slot": 1} trailing"#,
            r#"{"slot": 1"#,
        ] {
            assert!(parse_flat_json_object(json).is_err(), "accepted {json}");
        }
    }

    #[test]
    fn rejects_a_mistyped_field() {
        let mistyped = SNAPSHOT_JSON.replace("\"executable\": true", "\"executable\": 1");
        assert!(parse_account_snapshot(&mistyped)
            .unwrap_err()
            .contains("expected a boolean"));
    }
}
