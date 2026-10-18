//! A small BSON reader (bsonspec.org) for the client's content tables (data\contentdata\*.bson): documents, arrays,
//! strings, numbers and booleans.

use std::io;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Double(f64),
    Text(String),
    /// A document or an array (an array's keys are "0", "1", ...), in file order.
    Document(Vec<(String, Value)>),
    Bool(bool),
    Null,
    Int(i64),
}

impl Value {
    /// The value under `key`, if this is a document (or array) that has it.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Document(fields) => fields.iter().find(|(name, _)| name == key).map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn entries(&self) -> &[(String, Value)] {
        match self {
            Value::Document(fields) => fields,
            _ => &[],
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(value) => Some(*value),
            Value::Double(value) => Some(*value as i64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("BSON: {what}"))
}

/// Reads a whole file: one document.
pub fn parse(bytes: &[u8]) -> io::Result<Value> {
    let (value, _) = document(bytes, 0)?;
    Ok(value)
}

fn take<const N: usize>(bytes: &[u8], at: usize) -> io::Result<[u8; N]> {
    bytes.get(at..at + N).and_then(|slice| slice.try_into().ok()).ok_or_else(|| invalid("cut short"))
}

fn document(bytes: &[u8], start: usize) -> io::Result<(Value, usize)> {
    let length = i32::from_le_bytes(take(bytes, start)?) as usize;
    let end = start.checked_add(length).filter(|&end| end <= bytes.len() && length >= 5).ok_or_else(|| invalid("bad document length"))?;
    let mut at = start + 4;
    let mut fields = Vec::new();
    while at < end - 1 {
        let kind = bytes[at];
        let name_end = bytes[at + 1..end].iter().position(|&byte| byte == 0).ok_or_else(|| invalid("unterminated name"))? + at + 1;
        let name = String::from_utf8_lossy(&bytes[at + 1..name_end]).into_owned();
        at = name_end + 1;
        let value = match kind {
            0x01 => {
                at += 8;
                Value::Double(f64::from_le_bytes(take(bytes, at - 8)?))
            }
            0x02 => {
                let length = i32::from_le_bytes(take(bytes, at)?) as usize;
                let text = bytes.get(at + 4..at + 4 + length.saturating_sub(1)).ok_or_else(|| invalid("string cut short"))?;
                at += 4 + length;
                Value::Text(String::from_utf8_lossy(text).into_owned())
            }
            0x03 | 0x04 => {
                let (value, next) = document(bytes, at)?;
                at = next;
                value
            }
            0x08 => {
                at += 1;
                Value::Bool(take::<1>(bytes, at - 1)?[0] != 0)
            }
            0x0A => Value::Null,
            0x10 => {
                at += 4;
                Value::Int(i64::from(i32::from_le_bytes(take(bytes, at - 4)?)))
            }
            0x12 => {
                at += 8;
                Value::Int(i64::from_le_bytes(take(bytes, at - 8)?))
            }
            other => return Err(invalid(&format!("element type {other:#04x}"))),
        };
        fields.push((name, value));
    }
    Ok((Value::Document(fields), end))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document from its encoded elements.
    fn doc(elements: &[u8]) -> Vec<u8> {
        let mut bytes = ((elements.len() + 5) as i32).to_le_bytes().to_vec();
        bytes.extend(elements);
        bytes.push(0);
        bytes
    }

    #[test]
    fn reads_documents_arrays_and_scalars() {
        let mut array = vec![0x10, b'0', 0];
        array.extend(7i32.to_le_bytes());
        let mut elements = vec![0x02];
        elements.extend(b"Name\0");
        elements.extend(4i32.to_le_bytes());
        elements.extend(b"Orc\0");
        elements.extend([0x04]);
        elements.extend(b"List\0");
        elements.extend(doc(&array));
        elements.extend([0x12]);
        elements.extend(b"Big\0");
        elements.extend((-5_000_000_000i64).to_le_bytes());
        elements.extend([0x08]);
        elements.extend(b"On\0");
        elements.push(1);
        let value = parse(&doc(&elements)).unwrap();
        assert_eq!(value.get("Name").and_then(Value::as_str), Some("Orc"));
        assert_eq!(value.get("List").and_then(|list| list.get("0")).and_then(Value::as_int), Some(7));
        assert_eq!(value.get("Big").and_then(Value::as_int), Some(-5_000_000_000));
        assert_eq!(value.get("On"), Some(&Value::Bool(true)));
        assert_eq!(value.entries().len(), 4);
    }

    #[test]
    fn refuses_broken_files() {
        assert!(parse(&[1, 2]).is_err());
        assert!(parse(&doc(&[0x02, b'a', 0, 9, 0, 0, 0, b'x'])).is_err());
        assert!(parse(&doc(&[0x7F, b'a', 0])).is_err());
    }
}
