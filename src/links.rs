//! Links in chat and NPC text. The three item forms the server sends (`<ITEM>name<INFO>id</INFO></ITEM>`,
//! `<ITEMLINK>…</ITEMLINK>` and `<ITEML>…</ITEML>`, see rathena's `create_item_link`) and navigation links
//! (`<NAVI>name<INFO>map,x,y,…</INFO></NAVI>`) become marked runs in the text the windows already draw: the link's
//! words between markers that carry its target, so a line keeps its links through wrapping and the windows can
//! underline them and take clicks on them.

use std::ops::Range;

/// Opens a link; the target follows, then `WORDS`, then the words shown, then `END`.
const START: char = '\u{E000}';
const WORDS: char = '\u{E001}';
const END: char = '\u{E002}';
/// Wrapped text: per line, its pieces' colour, text and the link each is part of.
pub type LinkedLines = Vec<Vec<([u8; 3], String, Option<Target>)>>;
/// The colours item links and navigation links are drawn in.
pub const COLOR: [u8; 3] = [0x00, 0x70, 0xC0];
pub const NAVI_COLOR: [u8; 3] = [0xC0, 0x00, 0x00];

/// What a link opens: an item's information, or a navigation to a place (x and y 0: the map itself).
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Item(ItemLink),
    Navi { map: String, cell: [u16; 2] },
}

impl Target {
    pub fn color(&self) -> [u8; 3] {
        match self {
            Target::Item(_) => COLOR,
            Target::Navi { .. } => NAVI_COLOR,
        }
    }

    pub fn item(&self) -> Option<&ItemLink> {
        match self {
            Target::Item(item) => Some(item),
            Target::Navi { .. } => None,
        }
    }

    fn encode(&self) -> String {
        match self {
            Target::Item(item) => format!("i {}", item.encode()),
            Target::Navi { map, cell } => format!("n {} {} {map}", cell[0], cell[1]),
        }
    }

    fn decode(text: &str) -> Option<Self> {
        match text.split_once(' ')? {
            ("i", item) => ItemLink::decode(item).map(Target::Item),
            ("n", place) => {
                let mut fields = place.splitn(3, ' ');
                let cell = [fields.next()?.parse().ok()?, fields.next()?.parse().ok()?];
                Some(Target::Navi { map: fields.next()?.to_owned(), cell })
            }
            _ => None,
        }
    }
}

/// The item a link stands for: what the item information window shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemLink {
    pub id: u32,
    pub refine: u8,
    pub cards: [u32; 4],
    pub grade: u8,
    /// Random options: id, parameter, value.
    pub options: Vec<[u32; 3]>,
}

impl ItemLink {
    fn encode(&self) -> String {
        let options: Vec<String> = self.options.iter().map(|option| format!("{}.{}.{}", option[0], option[1], option[2])).collect();
        format!("{} {} {} {} {} {} {} {}", self.id, self.refine, self.cards[0], self.cards[1], self.cards[2], self.cards[3], self.grade, options.join(","))
    }

    fn decode(text: &str) -> Option<Self> {
        let mut fields = text.split(' ');
        let mut number = || fields.next()?.parse::<u32>().ok();
        let id = number()?;
        let refine = number()? as u8;
        let cards = [number()?, number()?, number()?, number()?];
        let grade = number()? as u8;
        let options = fields
            .next()
            .unwrap_or_default()
            .split(',')
            .filter(|option| !option.is_empty())
            .filter_map(|option| {
                let values: Vec<u32> = option.split('.').filter_map(|value| value.parse().ok()).collect();
                <[u32; 3]>::try_from(values).ok()
            })
            .collect();
        Some(Self { id, refine, cards, grade, options })
    }
}

/// rathena's base62 digits (`base62_dictionary`).
fn base62(text: &str) -> Option<u32> {
    text.chars().try_fold(0u32, |value, digit| {
        let digit = match digit {
            '0'..='9' => digit as u32 - '0' as u32,
            'a'..='z' => digit as u32 - 'a' as u32 + 10,
            'A'..='Z' => digit as u32 - 'A' as u32 + 36,
            _ => return None,
        };
        value.checked_mul(62)?.checked_add(digit)
    })
}

/// An `<ITEML>` body (PACKETVER 20200724 and up): five digits of equip location, the equip flag, the item id, then
/// `%` refine, `&` look, `'` grade, a `)` before each of the four cards, and per random option `+` id, `,` parameter,
/// `-` value. Every field but the first two and the id is optional.
pub fn decode_iteml(body: &str) -> Option<ItemLink> {
    let rest = body.get(6..)?;
    let marks = ['%', '&', '\'', ')', '+', ',', '-'];
    let id_end = rest.find(marks).unwrap_or(rest.len());
    let mut link = ItemLink { id: base62(&rest[..id_end])?, ..ItemLink::default() };
    let mut cards = 0;
    let mut option = [0; 3];
    let mut rest = &rest[id_end..];
    while let Some(mark) = rest.chars().next() {
        let end = rest[1..].find(marks).map_or(rest.len(), |at| at + 1);
        let value = base62(&rest[1..end])?;
        match mark {
            '%' => link.refine = value as u8,
            '\'' => link.grade = value as u8,
            ')' if cards < 4 => {
                link.cards[cards] = value;
                cards += 1;
            }
            '+' => option[0] = value,
            ',' => option[1] = value,
            '-' => {
                option[2] = value;
                link.options.push(option);
            }
            _ => {}
        }
        rest = &rest[end..];
    }
    Some(link)
}

/// `words` as a link to `target`.
pub fn marked(words: &str, target: &Target) -> String {
    format!("{START}{}{WORDS}{words}{END}", target.encode())
}

/// The text with its item and navigation links made into marked runs, items named by `name_of` where the link carries
/// no words (`<ITEML>`).
pub fn mark(text: &str, name_of: &dyn Fn(&ItemLink) -> String) -> String {
    let mut shown = String::new();
    let mut rest = text;
    'scan: while let Some(open) = rest.find('<') {
        for tag in ["ITEMLINK", "ITEML", "ITEM", "NAVI"] {
            let start = format!("<{tag}>");
            let end = format!("</{tag}>");
            if let Some(inner) = rest[open..].strip_prefix(start.as_str())
                && let Some(close) = inner.find(end.as_str())
            {
                let body = &inner[..close];
                let info = || body.split_once("<INFO>").map(|(words, info)| (words, info.trim_end_matches("</INFO>").trim()));
                let link = match tag {
                    "ITEML" => decode_iteml(body).map(|item| (name_of(&item), Target::Item(item))),
                    // name<INFO>map,x,y,…</INFO>
                    "NAVI" => info().and_then(|(words, info)| {
                        let mut fields = info.split(',');
                        let map = fields.next()?.trim().to_owned();
                        let cell = [fields.next()?.trim().parse().ok()?, fields.next()?.trim().parse().ok()?];
                        Some((words.to_owned(), Target::Navi { map, cell }))
                    }),
                    // name<INFO>id</INFO>
                    _ => info().and_then(|(words, id)| Some((words.to_owned(), Target::Item(ItemLink { id: id.parse().ok()?, ..ItemLink::default() })))),
                };
                let Some((words, item)) = link else { continue };
                shown.push_str(&rest[..open]);
                shown.push_str(&marked(&words, &item));
                rest = &inner[close + end.len()..];
                continue 'scan;
            }
        }
        shown.push_str(&rest[..=open]);
        rest = &rest[open + 1..];
    }
    shown.push_str(rest);
    shown
}

/// The text as shown (markers and targets taken out), and each link's byte range in it with its target.
pub fn split(text: &str) -> (String, Vec<(Range<usize>, Target)>) {
    let mut shown = String::new();
    let mut runs = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find(START) {
        shown.push_str(&rest[..open]);
        let after = &rest[open + START.len_utf8()..];
        let (Some(words_at), Some(end_at)) = (after.find(WORDS), after.find(END)) else { break };
        if let Some(item) = Target::decode(&after[..words_at]) {
            let start = shown.len();
            shown.push_str(&after[words_at + WORDS.len_utf8()..end_at]);
            runs.push((start..shown.len(), item));
        }
        rest = &after[end_at + END.len_utf8()..];
    }
    shown.push_str(rest.trim_start_matches([START, WORDS, END]));
    (shown, runs)
}

/// The link whose box (x, y, width, height) holds `cursor`, the last drawn first.
pub fn at(boxes: &[([f32; 4], Target)], cursor: [f32; 2]) -> Option<&Target> {
    boxes
        .iter()
        .rev()
        .find(|(area, _)| (area[0]..area[0] + area[2]).contains(&cursor[0]) && (area[1]..area[1] + area[3]).contains(&cursor[1]))
        .map(|(_, item)| item)
}

/// Pieces of marked text, in order: each piece's text and the link it's part of, if any.
pub fn pieces(text: &str) -> Vec<(String, Option<Target>)> {
    let (shown, runs) = split(text);
    let mut pieces = Vec::new();
    let mut at = 0;
    for (range, item) in runs {
        if range.start > at {
            pieces.push((shown[at..range.start].to_owned(), None));
        }
        pieces.push((shown[range.clone()].to_owned(), Some(item)));
        at = range.end;
    }
    if at < shown.len() || pieces.is_empty() {
        pieces.push((shown[at..].to_owned(), None));
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(item: &ItemLink) -> String {
        if item.id == 1201 { "Knife".into() } else { format!("item {}", item.id) }
    }

    #[test]
    fn reads_each_link_form() {
        // An NPC's and our own Shift-click's: the words and the id.
        let (shown, runs) = split(&mark("buy an <ITEM>[Apple]<INFO>512</INFO></ITEM> now", &named));
        assert_eq!(shown, "buy an [Apple] now");
        assert_eq!(runs, vec![(7..14, Target::Item(ItemLink { id: 512, ..ItemLink::default() }))]);
        let (shown, runs) = split(&mark("<ITEMLINK>Jellopy<INFO>909</INFO></ITEMLINK>", &named));
        assert_eq!((shown.as_str(), runs[0].1.item().map(|item| item.id)), ("Jellopy", Some(909)));
        // rathena's <ITEML> for a +7 Knife (1201 = "jn") with cards 4001 (12x) and 0, grade 1, one option 1/0/20.
        let iteml = "<ITEML>000021jn%07&01'01)12x)00)00)00+01,00-0k</ITEML>";
        let (shown, runs) = split(&mark(&format!("look {iteml}!"), &named));
        assert_eq!(shown, "look Knife!");
        let item = ItemLink { id: 1201, refine: 7, cards: [4001, 0, 0, 0], grade: 1, options: vec![[1, 0, 20]] };
        assert_eq!(runs, vec![(5..10, Target::Item(item))]);
        // A navigation link: its words and its place.
        let (shown, runs) = split(&mark("go to <NAVI>[Tool Shop]<INFO>prontera,134,221,0,000,0</INFO></NAVI>.", &named));
        assert_eq!(shown, "go to [Tool Shop].");
        assert_eq!(runs, vec![(6..17, Target::Navi { map: "prontera".into(), cell: [134, 221] })]);
        assert_eq!(runs[0].1.color(), NAVI_COLOR);
        // A plain item with nothing after its id, and text that only looks like a tag.
        assert_eq!(decode_iteml("0000000jn").map(|item| item.id), Some(1201));
        assert_eq!(split(&mark("a < b <ITEM>broken</ITEM>", &named)).0, "a < b <ITEM>broken</ITEM>");
    }

    #[test]
    fn splits_text_into_pieces() {
        let item = |id: u32| Target::Item(ItemLink { id, ..ItemLink::default() });
        let text = format!("a {} b {}", marked("One", &item(1)), marked("Two", &item(2)));
        let parts = pieces(&text);
        let words: Vec<(&str, Option<u32>)> =
            parts.iter().map(|(text, target)| (text.as_str(), target.as_ref().and_then(Target::item).map(|item| item.id))).collect();
        assert_eq!(words, [("a ", None), ("One", Some(1)), (" b ", None), ("Two", Some(2))]);
        assert_eq!(pieces("plain"), [("plain".to_owned(), None)]);
    }
}
