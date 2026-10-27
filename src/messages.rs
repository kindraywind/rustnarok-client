//! The client's numbered UI and system messages (data\msgstringtable.csv, or a translation's msgstringtable.txt).

use std::fmt::Display;
use std::io;

use crate::grf::Grf;

const TABLE: &str = "data\\msgstringtable.csv";
const PLAIN_TABLE: &str = "data\\msgstringtable.txt";

/// Message ID 106: the inventory window's title.
pub const INVENTORY: usize = 106;
/// "%s %d 개 획득": an item was picked up.
pub const GOT_ITEM: usize = 153;
/// "%s : %d 개": an item and how many.
pub const ITEM_AMOUNT: usize = 183;
/// "%s [%d Option] : %d Quantity": an identified item with random options, how many, and how many of it.
pub const ITEM_OPTIONS_AMOUNT: usize = 2551;
/// "%s [%d Option]": an identified item with random options, and how many it has.
pub const ITEM_OPTIONS: usize = 2600;
/// Picking up failed: too heavy, or anything else.
pub const CANT_CARRY_WEIGHT: usize = 52;
pub const CANT_CARRY: usize = 53;

/// Messages by ID, the line number. In the csv each line holds a base64 key name and base64 UTF-8 text.
pub fn load(grf: &Grf) -> io::Result<Vec<String>> {
    // The older plain table (one message a line, each ending in #) is what translations ship; it goes first.
    if let Ok(bytes) = grf.read(PLAIN_TABLE) {
        return Ok(plain(&bytes));
    }
    let bytes = grf.read(TABLE)?;
    let table = std::str::from_utf8(&bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(table
        .lines()
        .map(|line| {
            let text = line.split_once(',').map_or("", |(_, text)| text);
            base64(text).and_then(|bytes| String::from_utf8(bytes).ok()).unwrap_or_default()
        })
        .collect())
}

/// Messages from msgstringtable.txt: EUC-KR lines ending in `#`.
fn plain(bytes: &[u8]) -> Vec<String> {
    let text = encoding_rs::EUC_KR.decode_without_bom_handling(bytes).0;
    text.lines().map(|line| line.trim_end_matches('\r').trim_end_matches('#').to_owned()).collect()
}

fn base64(text: &str) -> Option<Vec<u8>> {
    let digit = |byte: u8| match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let digits: Vec<u8> = text.trim_end().bytes().take_while(|&byte| byte != b'=').map(digit).collect::<Option<_>>()?;
    // Each group of n digits (n = 2..4) carries n - 1 bytes.
    Some(
        digits
            .chunks(4)
            .flat_map(|chunk| {
                let bits = chunk.iter().fold(0u32, |bits, &digit| bits << 6 | u32::from(digit)) << (6 * (4 - chunk.len()));
                bits.to_be_bytes()[1..chunk.len()].to_vec()
            })
            .collect(),
    )
}

/// Fills a message's printf-style placeholders (`%s`, `%d`, `%3d`, ...) with `args` in order.
pub fn fill(template: &str, args: &[&dyn Display]) -> String {
    let mut filled = String::new();
    let mut args = args.iter();
    let mut chars = template.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '%' {
            filled.push(character);
            continue;
        }
        let mut width = String::new();
        while let Some(digit) = chars.next_if(char::is_ascii_digit) {
            width.push(digit);
        }
        match chars.next() {
            Some('s' | 'd' | 'u' | 'i') => {
                let value = args.next().map(ToString::to_string).unwrap_or_default();
                filled.push_str(&format!("{value:>width$}", width = width.parse().unwrap_or(0)));
            }
            Some('%') => filled.push('%'),
            other => {
                filled.push('%');
                filled.push_str(&width);
                filled.extend(other);
            }
        }
    }
    filled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64() {
        assert_eq!(base64("TVNJX0lURU0=").unwrap(), b"MSI_ITEM");
        assert_eq!(base64("YWI=").unwrap(), b"ab");
        assert_eq!(base64("YQ==").unwrap(), b"a");
    }

    #[test]
    fn fills_placeholders() {
        assert_eq!(fill("%s : %d 개", &[&"젤로피", &3]), "젤로피 : 3 개");
        assert_eq!(fill("Weight : %3d / %3d", &[&7, &1200]), "Weight :   7 / 1200");
        assert_eq!(fill("100%%", &[]), "100%");
    }

    /// Needs the official data.grf, at `RUSTNAROK_CLIENT_GRF` or `./data.grf`.
    #[test]
    fn reads_the_client_messages() {
        let Some(grf) = Grf::for_tests() else { return };
        let messages = load(&grf).unwrap();
        assert_eq!(messages[INVENTORY], "소지아이템");
        assert_eq!(messages[ITEM_AMOUNT], "%s : %d 개");
    }
}
