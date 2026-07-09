//! A 64-bit machine word on the wire.
//!
//! JSON numbers are IEEE doubles. They represent integers exactly only up to
//! 2^53, and a register can hold any of 2^64 bit patterns. `mov rax, -1` puts
//! `0xffffffffffffffff` in `rax`; as a JSON number that becomes
//! `18446744073709552000`, which is not the value, and a tool whose entire
//! purpose is showing exact bytes must not do that.
//!
//! So every machine word crosses the wire as a `0x`-prefixed hex string. This
//! is also simply how a human wants to read a register.
//!
//! Input is tolerant — a JSON number, a decimal string, or `0x`-prefixed hex
//! all deserialise — because being strict about input buys nothing here.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct U64(pub u64);

impl From<u64> for U64 {
    fn from(v: u64) -> U64 {
        U64(v)
    }
}

impl Serialize for U64 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("{:#x}", self.0))
    }
}

impl<'de> Deserialize<'de> for U64 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<U64, D::Error> {
        d.deserialize_any(U64Visitor)
    }
}

struct U64Visitor;

impl Visitor<'_> for U64Visitor {
    type Value = U64;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a 64-bit integer, as a number or a decimal/0x-hex string")
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<U64, E> {
        Ok(U64(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<U64, E> {
        Ok(U64(v as u64))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<U64, E> {
        // Reject a float that cannot be an exact integer, rather than
        // truncating it and pretending we understood.
        if v.fract() != 0.0 || v < 0.0 || v > (1u64 << 53) as f64 {
            return Err(E::custom(format!(
                "{v} is not an exact integer below 2^53; send it as a hex string"
            )));
        }
        Ok(U64(v as u64))
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<U64, E> {
        let t = s.trim();
        let parsed = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            Some(hex) => u64::from_str_radix(hex, 16),
            None => t.parse::<u64>(),
        };
        parsed.map(U64).map_err(|_| E::custom(format!("`{s}` is not a 64-bit integer")))
    }
}

/// Decode a hex byte string. Tolerates whitespace, commas and `0x` prefixes.
pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    lesson::from_hex(s)
}

pub fn to_hex(bytes: &[u8]) -> String {
    lesson::to_hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_width_register_survives_the_round_trip() {
        let v = U64(0xffff_ffff_ffff_ffff);
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"0xffffffffffffffff\"");
        assert_eq!(serde_json::from_str::<U64>(&json).unwrap(), v);
    }

    #[test]
    fn a_json_number_that_a_double_cannot_hold_is_refused_not_truncated() {
        // This is the bug the hex encoding exists to prevent: as a JSON number,
        // u64::MAX arrives as 18446744073709552000.
        let e = serde_json::from_str::<U64>("18446744073709551615");
        // serde_json parses this as u64 exactly, so it is accepted...
        assert_eq!(e.unwrap(), U64(u64::MAX));
        // ...but a float literal that lost precision is rejected.
        assert!(serde_json::from_str::<U64>("1.8446744073709552e19").is_err());
    }

    #[test]
    fn input_accepts_numbers_decimal_strings_and_hex_strings() {
        assert_eq!(serde_json::from_str::<U64>("4096").unwrap(), U64(4096));
        assert_eq!(serde_json::from_str::<U64>("\"4096\"").unwrap(), U64(4096));
        assert_eq!(serde_json::from_str::<U64>("\"0x1000\"").unwrap(), U64(4096));
    }
}
