//! The client's server list, clientinfo.xml (sclientinfo.xml for test-server clients): for each connection its name,
//! login server, client version and loading pictures.

use crate::grf::Grf;

/// The server list in the data folder: its loose clientinfo.xml, else sclientinfo.xml. Never the archive's copy
/// (official data carries one for its own servers), so the client only goes where the player's files say.
pub fn load(grf: &Grf) -> Vec<Connection> {
    ["data\\clientinfo.xml", "data\\sclientinfo.xml"].iter().find_map(|path| grf.read_loose(path).ok()).map(|bytes| parse(&bytes)).unwrap_or_default()
}

/// One `<connection>` of the list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Connection {
    pub display: String,
    pub address: String,
    pub port: u16,
    /// What the login packet says the client's version is.
    pub version: u32,
    /// The service's language, which picks the code page of chat and names (`codepage`).
    pub langtype: u8,
    /// Loading pictures, by file name in the UI texture folder.
    pub loading: Vec<String>,
}

impl Connection {
    /// The login server, `address:port`.
    pub fn server(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }
}

/// The connections in the file (EUC-KR XML), in order. Ones commented out (`<!-- -->`) don't count, and neither do
/// ones without an address.
pub fn parse(bytes: &[u8]) -> Vec<Connection> {
    let text = encoding_rs::EUC_KR.decode(bytes).0;
    let mut kept = String::new();
    let mut rest = text.as_ref();
    while let Some(start) = rest.find("<!--") {
        kept.push_str(&rest[..start]);
        rest = rest[start..].find("-->").map_or("", |end| &rest[start + end + 3..]);
    }
    kept.push_str(rest);
    elements(&kept, "connection")
        .filter_map(|connection| {
            let field = |name: &str| elements(connection, name).next().map(|value| value.trim().to_owned());
            let loading = elements(connection, "loading").flat_map(|list| elements(list, "image")).map(|image| image.trim().to_owned());
            Some(Connection {
                display: field("display").unwrap_or_default(),
                address: field("address").filter(|address| !address.is_empty())?,
                port: field("port").and_then(|port| port.parse().ok()).unwrap_or(6900),
                version: field("version").and_then(|version| version.parse().ok()).unwrap_or(0),
                langtype: field("langtype").and_then(|langtype| langtype.parse().ok()).unwrap_or(0),
                loading: loading.filter(|image| !image.is_empty()).collect(),
            })
        })
        .collect()
}

/// The code page a langtype's service writes text in: Korea's, Japan's, China's, Taiwan's, Thailand's, Russia's,
/// Vietnam's, the UAE's, 160-164 for other Windows code pages, 240 for UTF-8, and Latin for the rest.
pub fn codepage(langtype: u8) -> &'static encoding_rs::Encoding {
    use encoding_rs::*;
    match langtype {
        0 => EUC_KR,
        2 => SHIFT_JIS,
        3 => GBK,
        4 => BIG5,
        5 => WINDOWS_874,
        14 => WINDOWS_1251,
        15 => WINDOWS_1258,
        19 => WINDOWS_1256,
        160 => WINDOWS_1250,
        161 => WINDOWS_1253,
        162 => WINDOWS_1254,
        163 => WINDOWS_1255,
        164 => WINDOWS_1257,
        240 => UTF_8,
        _ => WINDOWS_1252,
    }
}

/// The contents of each `<name>...</name>` in `text`, in order.
fn elements<'a>(text: &'a str, name: &str) -> impl Iterator<Item = &'a str> {
    let (open, close) = (format!("<{name}>"), format!("</{name}>"));
    let mut rest = text;
    std::iter::from_fn(move || {
        let start = rest.find(&open)? + open.len();
        let end = start + rest[start..].find(&close)?;
        let inside = &rest[start..end];
        rest = &rest[end + close.len()..];
        Some(inside)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_connections_and_skips_commented_ones() {
        let mut file = b"<?xml version=\"1.0\" encoding=\"euc-kr\" ?>\n<clientinfo>\n<connection>\n<display>".to_vec();
        file.extend_from_slice(&encoding_rs::EUC_KR.encode("로컬").0);
        file.extend_from_slice(
            b"</display>\n  <address> 127.0.0.1 </address>\n  <port>6900</port>\n  <version>55</version>\n\
            <loading>\n<image>loading00.jpg</image>\n<image>loading01.jpg</image>\n</loading>\n</connection>\n\
            <!--\n<connection><display>Old</display><address>10.0.0.1</address></connection>\n-->\n\
            <connection><display>Test</display><address>example.test</address><langtype>5</langtype></connection>\n\
            <connection><display>Broken</display></connection>\n</clientinfo>\n",
        );
        let connections = parse(&file);
        assert_eq!(connections.len(), 2);
        assert_eq!(connections[0].display, "로컬");
        assert_eq!((connections[0].server().as_str(), connections[0].version), ("127.0.0.1:6900", 55));
        assert_eq!(connections[0].loading, ["loading00.jpg", "loading01.jpg"]);
        assert_eq!((connections[1].server().as_str(), connections[1].version), ("example.test:6900", 0));
        assert_eq!((connections[0].langtype, connections[1].langtype), (0, 5));
        assert!(parse(b"<?xml version=\"1.0\" ?><no_free_server></no_free_server>").is_empty());
    }

    #[test]
    fn langtype_picks_the_code_page() {
        let thai = codepage(5).encode("สวัสดี").0;
        assert_eq!(thai.as_ref(), [0xCA, 0xC7, 0xD1, 0xCA, 0xB4, 0xD5]);
        assert_eq!(codepage(2).encode("気").0.as_ref(), [0x8B, 0x43]);
        assert_eq!(
            [codepage(0), codepage(1), codepage(14), codepage(240), codepage(99)].map(|page| page.name()),
            ["EUC-KR", "windows-1252", "windows-1251", "UTF-8", "windows-1252"]
        );
    }
}
