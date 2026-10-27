//! Links in chat and NPC text. The three item forms the server sends (`<ITEM>name<INFO>id</INFO></ITEM>`,
//! `<ITEMLINK>…</ITEMLINK>` and `<ITEML>…</ITEML>`, see rathena's `create_item_link`) and navigation links
//! (`<NAVI>name<INFO>map,x,y,…</INFO></NAVI>`), web links (`<URL>`) and `<QUEST>`/`<TIPBOX>` become marked runs in the text the windows already draw: the link's
//! words between markers that carry its target, so a line keeps its links through wrapping and the windows can
//! underline them and take clicks on them.

use std::ops::Range;

/// Opens a link; the target follows, then `WORDS`, then the words shown, then `END`.
const START: char = '\u{E000}';
const WORDS: char = '\u{E001}';
const END: char = '\u{E002}';
/// Wrapped text: per line, its pieces' colour, text and the link each is part of.
pub type LinkedLines = Vec<Vec<([u8; 3], String, Option<Target>)>>;
/// The colours item links and navigation links are drawn in, in the chat.
pub const COLOR: [u8; 3] = [0x00, 0x70, 0xC0];
pub const NAVI_COLOR: [u8; 3] = [0xC0, 0x00, 0x00];
/// In NPC text: web links, and every other link.
pub const URL_COLOR: [u8; 3] = [0x00, 0xA3, 0xE6];
pub const NPC_COLOR: [u8; 3] = [0x4A, 0x93, 0xF4];

/// What a link opens: an item's information, a navigation to a place (x and y 0: the map itself), a web page (the
/// address as the script gave it), a quest in the quest window, or nothing yet (`<TIPBOX>`, a `<QUEST>` without an id).
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Item(ItemLink),
    Navi { map: String, cell: [u16; 2] },
    Url(String),
    Quest(u32),
    // ponytail: a tip link does nothing when clicked until that window exists.
    Tip,
}

impl Target {
    /// In the chat.
    pub fn color(&self) -> [u8; 3] {
        match self {
            Target::Item(_) => COLOR,
            Target::Navi { .. } => NAVI_COLOR,
            Target::Url(_) => URL_COLOR,
            Target::Quest(_) | Target::Tip => NPC_COLOR,
        }
    }

    /// In NPC text.
    pub fn npc_color(&self) -> [u8; 3] {
        if matches!(self, Target::Url(_)) { URL_COLOR } else { NPC_COLOR }
    }

    pub fn item(&self) -> Option<&ItemLink> {
        match self {
            Target::Item(item) => Some(item),
            _ => None,
        }
    }

    fn encode(&self) -> String {
        match self {
            Target::Item(item) => format!("i {}", item.encode()),
            Target::Navi { map, cell } => format!("n {} {} {map}", cell[0], cell[1]),
            Target::Url(address) => format!("u {address}"),
            Target::Quest(id) => format!("q {id}"),
            Target::Tip => "t -".into(),
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
            ("u", address) => Some(Target::Url(address.to_owned())),
            ("q", id) => id.parse().ok().map(Target::Quest),
            ("t", _) => Some(Target::Tip),
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
    /// Equipment, as an `<ITEML>` says (the other forms don't).
    pub equip: bool,
    /// Random options: id, parameter, value.
    pub options: Vec<[u32; 3]>,
}

impl ItemLink {
    fn encode(&self) -> String {
        let options: Vec<String> = self.options.iter().map(|option| format!("{}.{}.{}", option[0], option[1], option[2])).collect();
        let [id, refine, grade, equip] = [self.id, u32::from(self.refine), u32::from(self.grade), u32::from(self.equip)];
        format!("{id} {refine} {} {} {} {} {grade} {equip} {}", self.cards[0], self.cards[1], self.cards[2], self.cards[3], options.join(","))
    }

    fn decode(text: &str) -> Option<Self> {
        let mut fields = text.split(' ');
        let mut number = || fields.next()?.parse::<u32>().ok();
        let id = number()?;
        let refine = number()? as u8;
        let cards = [number()?, number()?, number()?, number()?];
        let grade = number()? as u8;
        let equip = number()? == 1;
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
        Some(Self { id, refine, cards, grade, equip, options })
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
    let mut link = ItemLink { id: base62(&rest[..id_end])?, equip: body.get(5..6) == Some("1"), ..ItemLink::default() };
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

/// The page a web link opens: its address up to the first comma (rathena's scripts add ",1024,768"), when it is
/// http or https.
// ponytail: the comma cut and the two schemes, until the game's rules are known.
pub fn page(address: &str) -> Option<&str> {
    let page = address.split(',').next().unwrap_or_default().trim();
    (page.starts_with("http://") || page.starts_with("https://")).then_some(page)
}

/// `words` as a link to `target`.
pub fn marked(words: &str, target: &Target) -> String {
    format!("{START}{}{WORDS}{words}{END}", target.encode())
}

/// Whether the text holds an item or navigation link (the chat keeps such a line whole).
pub fn has_links(text: &str) -> bool {
    ["<ITEMLINK>", "<ITEML>", "<ITEM>", "<NAVI>"].iter().any(|tag| text.contains(tag))
}

/// The text with its item and navigation links made into marked runs, items named by `name_of` where the link carries
/// no words (`<ITEML>`).
pub fn mark(text: &str, name_of: &dyn Fn(&ItemLink) -> String) -> String {
    let mut shown = String::new();
    let mut rest = text;
    'scan: while let Some(open) = rest.find('<') {
        for tag in ["ITEMLINK", "ITEML", "ITEM", "NAVI", "URL", "QUEST", "TIPBOX"] {
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
                    // words<INFO>address</INFO>
                    "URL" => info().map(|(words, address)| (words.to_owned(), Target::Url(address.to_owned()))),
                    // words<INFO>quest id</INFO>; words<INFO>…</INFO>, or the words alone
                    "QUEST" | "TIPBOX" => match info().and_then(|(words, id)| Some((words, id.parse().ok()?))) {
                        Some((words, id)) if tag == "QUEST" => Some((words.to_owned(), Target::Quest(id))),
                        _ => Some((body.split("<INFO>").next().unwrap_or_default().to_owned(), Target::Tip)),
                    },
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
        let item = ItemLink { id: 1201, refine: 7, cards: [4001, 0, 0, 0], grade: 1, equip: true, options: vec![[1, 0, 20]] };
        assert_eq!(runs, vec![(5..10, Target::Item(item))]);
        // A navigation link: its words and its place.
        let (shown, runs) = split(&mark("go to <NAVI>[Tool Shop]<INFO>prontera,134,221,0,000,0</INFO></NAVI>.", &named));
        assert_eq!(shown, "go to [Tool Shop].");
        assert_eq!(runs, vec![(6..17, Target::Navi { map: "prontera".into(), cell: [134, 221] })]);
        assert_eq!(runs[0].1.color(), NAVI_COLOR);
        // A plain item with nothing after its id, and text that only looks like a tag.
        assert_eq!(decode_iteml("0000000jn").map(|item| (item.id, item.equip)), Some((1201, false)));
        assert_eq!(split(&mark("a < b <ITEM>broken</ITEM>", &named)).0, "a < b <ITEM>broken</ITEM>");
    }

    #[test]
    fn reads_web_quest_and_tip_links() {
        let (shown, runs) = split(&mark(
            "see <URL>our page<INFO>https://example.com/a,1024,768</INFO></URL> or <TIPBOX>[List]<INFO>9163</INFO></TIPBOX> and <QUEST>[Hunt]</QUEST> <QUEST>Kill<INFO>4275</INFO></QUEST>",
            &named,
        ));
        assert_eq!(shown, "see our page or [List] and [Hunt] Kill");
        let targets: Vec<&Target> = runs.iter().map(|(_, target)| target).collect();
        assert_eq!(targets, [&Target::Url("https://example.com/a,1024,768".into()), &Target::Tip, &Target::Tip, &Target::Quest(4275)]);
        // The address up to its first comma, web addresses only.
        assert_eq!(page("https://example.com/a,1024,768"), Some("https://example.com/a"));
        assert_eq!(page("http://example.com"), Some("http://example.com"));
        assert_eq!(page("file:///etc/passwd"), None);
        assert_eq!(page("javascript:alert(1)"), None);
        // NPC text draws web links in their own blue and every other link in another; the chat keeps its colours.
        assert_eq!([targets[0].npc_color(), targets[1].npc_color()], [URL_COLOR, NPC_COLOR]);
        let navi = Target::Navi { map: "prontera".into(), cell: [1, 2] };
        assert_eq!((navi.npc_color(), navi.color()), (NPC_COLOR, NAVI_COLOR));
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
