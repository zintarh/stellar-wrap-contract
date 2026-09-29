//! Completeness check for `error_messages.json`.
//!
//! The JSON file beside `errors.rs` is the user-facing catalog. This test
//! parses the `ContractError` enum from source so a new variant fails until
//! it has a message, a matching code, and an actionable flag.

use std::collections::{BTreeMap, BTreeSet};

use crate::ContractError;

struct Variant {
    name: String,
    code: u32,
}

struct MappedError {
    variant: String,
    code: u32,
    actionable: bool,
    message: String,
}

#[test]
fn every_variant_has_a_mapping() {
    let variants = contract_error_variants(include_str!("errors.rs"));
    let mapped = mapped_errors(include_str!("error_messages.json"));

    assert_eq!(variants.len() as u32, ContractError::VARIANT_COUNT);
    assert_eq!(variants.len(), ContractError::ALL_VARIANTS.len());
    for (parsed, variant) in variants.iter().zip(ContractError::ALL_VARIANTS) {
        assert_eq!(
            parsed.code,
            variant.code(),
            "{} code does not match ALL_VARIANTS",
            parsed.name
        );
    }

    let mut by_name = BTreeMap::new();
    let mut messages = BTreeSet::new();
    for entry in mapped {
        assert!(
            by_name.insert(entry.variant.clone(), entry).is_none(),
            "duplicate mapping"
        );
    }

    assert_eq!(
        by_name.len(),
        variants.len(),
        "mapping count does not match ContractError"
    );

    for variant in &variants {
        let entry = by_name.get(&variant.name).unwrap_or_else(|| {
            panic!(
                "missing user-facing mapping for ContractError::{}",
                variant.name
            )
        });
        assert_eq!(
            entry.code, variant.code,
            "{} mapping code does not match the enum discriminant",
            variant.name
        );
        assert!(
            !entry.message.trim().is_empty(),
            "{} message is empty",
            variant.name
        );
        assert!(
            messages.insert(entry.message.clone()),
            "duplicate message for {}",
            variant.name
        );
    }

    for name in by_name.keys() {
        assert!(
            variants.iter().any(|variant| variant.name == *name),
            "mapping lists unknown variant {name}"
        );
    }

    for name in [
        "WrapAlreadyExists",
        "InvalidPeriod",
        "Paused",
        "InvalidMerkleProof",
    ] {
        assert!(
            by_name[name].actionable,
            "{name} is something the user can act on"
        );
    }
    for name in [
        "AlreadyInitialized",
        "ArithmeticOverflow",
        "StorageInvariantViolation",
        "Unauthorized",
    ] {
        assert!(
            !by_name[name].actionable,
            "{name} is not something the user can act on"
        );
    }
}

fn contract_error_variants(source: &str) -> Vec<Variant> {
    let start = source
        .find("pub enum ContractError")
        .expect("ContractError enum");
    let relative = &source[start..];
    let open = relative.find('{').expect("enum body");
    let mut depth = 0;
    let mut end = None;
    for (index, byte) in relative.bytes().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(index);
                    break;
                }
            },
            _ => {},
        }
    }
    let end = end.expect("enum closing brace");
    let body = &relative[open + 1..end];
    let mut variants = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let Some((name, rest)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            continue;
        }
        let code_text: String = rest
            .trim()
            .trim_end_matches(',')
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect();
        let code = code_text
            .parse()
            .unwrap_or_else(|_| panic!("discriminant for {name}"));
        variants.push(Variant {
            name: name.to_string(),
            code,
        });
    }
    variants
}

fn mapped_errors(source: &str) -> Vec<MappedError> {
    let mut entries = Vec::new();
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'{' {
            index += 1;
            continue;
        }
        let start = index;
        let mut depth = 0;
        let mut in_string = false;
        let mut escaped = false;
        while index < bytes.len() {
            let byte = bytes[index];
            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
            } else if byte == b'"' {
                in_string = true;
            } else if byte == b'{' {
                depth += 1;
            } else if byte == b'}' {
                depth -= 1;
                if depth == 0 {
                    index += 1;
                    break;
                }
            }
            index += 1;
        }
        let object = &source[start..index];
        entries.push(MappedError {
            variant: json_string(object, "variant"),
            code: json_u32(object, "code"),
            actionable: json_bool(object, "actionable"),
            message: json_string(object, "message"),
        });
    }
    entries
}

fn json_string(object: &str, key: &str) -> String {
    let rest = value_after_key(object, key);
    assert!(rest.starts_with('"'), "expected a string for {key}");
    let mut value = String::new();
    let mut characters = rest[1..].chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            value.push(
                characters
                    .next()
                    .unwrap_or_else(|| panic!("escape in {key}")),
            );
        } else if character == '"' {
            return value;
        } else {
            value.push(character);
        }
    }
    panic!("unterminated string for {key}");
}

fn json_u32(object: &str, key: &str) -> u32 {
    let rest = value_after_key(object, key);
    let digits: String = rest
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("expected a number for {key}"))
}

fn json_bool(object: &str, key: &str) -> bool {
    let rest = value_after_key(object, key);
    if rest.starts_with("true") {
        true
    } else if rest.starts_with("false") {
        false
    } else {
        panic!("expected a boolean for {key}");
    }
}

fn value_after_key<'a>(object: &'a str, key: &str) -> &'a str {
    let pattern = format!("\"{key}\"");
    let at = object
        .find(&pattern)
        .unwrap_or_else(|| panic!("missing {key}"));
    let after_key = &object[at + pattern.len()..];
    let colon = after_key
        .find(':')
        .unwrap_or_else(|| panic!("missing colon after {key}"));
    after_key[colon + 1..].trim_start()
}
