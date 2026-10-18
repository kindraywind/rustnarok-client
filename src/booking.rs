//! Party booking through rathena's web server: posts are added, searched, listed and deleted over HTTP
//! (`/party/add`, `/party/search`, `/party/list`, `/party/del`), each request a multipart form carrying the account
//! and its web auth token from the login. Asking a post's leader to join goes over the map server (`net`).

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// What a post is for (rathena's `e_booking_purpose`; 0 means any in a search).
pub const PURPOSES: usize = 7;

/// A post on the board.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Post {
    pub account: u32,
    pub character: u32,
    pub name: String,
    pub purpose: u16,
    /// The roles it wants: tank, healer, damage dealer, assist.
    pub roles: [bool; 4],
    pub min_level: u16,
    pub max_level: u16,
    pub memo: String,
}

/// Who asks: the account, its web token, and the world (char server) name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Auth {
    pub account: u32,
    pub token: String,
    pub world: String,
}

/// A request to the board.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Our post (the leader's character in `post`).
    Add(Post),
    /// Posts whose level range is exactly this, for a purpose (0: any) and wanting any of these roles (none: any).
    Search { min_level: u16, max_level: u16, purpose: u16, roles: [bool; 4] },
    /// Every post, newest first.
    List,
    /// Our post goes.
    Delete,
}

/// What the board answered: done, or posts.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Done,
    Posts(Vec<Post>),
}

const ROLE_FIELDS: [&str; 4] = ["Tanker", "Healer", "Dealer", "Assist"];

impl Request {
    fn path(&self) -> &'static str {
        match self {
            Request::Add(_) => "/party/add",
            Request::Search { .. } => "/party/search",
            Request::List => "/party/list",
            Request::Delete => "/party/del",
        }
    }

    /// The form's fields, after the account's.
    fn fields(&self, auth: &Auth) -> Vec<(&'static str, String)> {
        let mut fields = vec![("AID", auth.account.to_string()), ("AuthToken", auth.token.clone()), ("WorldName", auth.world.clone())];
        match self {
            Request::Add(post) => {
                fields.extend([
                    ("GID", post.character.to_string()),
                    ("CharName", post.name.clone()),
                    ("Type", post.purpose.to_string()),
                    ("MinLV", post.min_level.to_string()),
                    ("MaxLV", post.max_level.to_string()),
                    ("Memo", post.memo.clone()),
                ]);
                fields.extend(ROLE_FIELDS.iter().zip(post.roles).map(|(&name, on)| (name, u8::from(on).to_string())));
            }
            Request::Search { min_level, max_level, purpose, roles } => {
                fields.extend([("MinLV", min_level.to_string()), ("MaxLV", max_level.to_string())]);
                if *purpose != 0 {
                    fields.push(("Type", purpose.to_string()));
                }
                fields.extend(ROLE_FIELDS.iter().zip(roles).filter(|(_, on)| **on).map(|(&name, _)| (name, "1".to_owned())));
            }
            Request::List | Request::Delete => {}
        }
        fields
    }
}

/// A multipart form with these fields, split by `boundary`.
pub fn form(fields: &[(&str, String)], boundary: &str) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes());
        body.extend(value.as_bytes());
        body.extend(b"\r\n");
    }
    body.extend(format!("--{boundary}--\r\n").as_bytes());
    body
}

/// The body of an HTTP response, if its status is 200.
pub fn response_body(response: &[u8]) -> io::Result<String> {
    let text = String::from_utf8_lossy(response);
    let (head, body) = text.split_once("\r\n\r\n").ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no HTTP header"))?;
    let status = head.split_whitespace().nth(1).unwrap_or_default();
    if status != "200" {
        return Err(io::Error::other(format!("the web server answered {status}")));
    }
    Ok(body.to_owned())
}

/// The posts in a reply (`{"Type": 1, "data": [...]}`, or one post under `data`); none when it has none.
pub fn posts(body: &str) -> io::Result<Vec<Post>> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if value["Type"].as_i64() != Some(1) {
        return Err(io::Error::other("the board refused"));
    }
    let entries = match &value["data"] {
        serde_json::Value::Array(entries) => entries.clone(),
        serde_json::Value::Null => Vec::new(),
        entry => vec![entry.clone()],
    };
    Ok(entries
        .iter()
        .map(|entry| {
            let number = |key: &str| entry[key].as_u64().unwrap_or(0);
            Post {
                account: number("AID") as u32,
                character: number("GID") as u32,
                name: entry["CharName"].as_str().unwrap_or_default().to_owned(),
                purpose: number("Type") as u16,
                roles: ROLE_FIELDS.map(|role| number(role) != 0),
                min_level: number("MinLV") as u16,
                max_level: number("MaxLV") as u16,
                memo: entry["Memo"].as_str().unwrap_or_default().to_owned(),
            }
        })
        .collect())
}

/// Sends a request to the web server at `address` (host:port) and reads its answer. It waits on the network: call it
/// off the frame.
pub fn send(address: &str, auth: &Auth, request: &Request) -> io::Result<Reply> {
    let boundary = "rustnarok-booking";
    let body = form(&request.fields(auth), boundary);
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let head = format!(
        "POST {} HTTP/1.1\r\nHost: {address}\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        request.path(),
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&body)?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let body = response_body(&response)?;
    match request {
        Request::Search { .. } | Request::List => Ok(Reply::Posts(posts(&body)?)),
        Request::Add(_) | Request::Delete => posts(&body).map(|_| Reply::Done),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> Auth {
        Auth { account: 2000001, token: "0123456789abcdef".into(), world: "rAthena".into() }
    }

    #[test]
    fn writes_the_forms() {
        let post = Post {
            character: 150004,
            name: "LaneA1".into(),
            purpose: 2,
            roles: [false, true, false, false],
            min_level: 1,
            max_level: 99,
            memo: "hi".into(),
            ..Post::default()
        };
        let fields = Request::Add(post).fields(&auth());
        let names: Vec<&str> = fields.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, ["AID", "AuthToken", "WorldName", "GID", "CharName", "Type", "MinLV", "MaxLV", "Memo", "Tanker", "Healer", "Dealer", "Assist"]);
        assert_eq!(fields[12], ("Assist", "0".to_owned()));
        assert_eq!(fields[10], ("Healer", "1".to_owned()));
        // A search names only what narrows it.
        let search = Request::Search { min_level: 1, max_level: 99, purpose: 0, roles: [false, true, false, false] }.fields(&auth());
        assert_eq!(search[3..], [("MinLV", "1".to_owned()), ("MaxLV", "99".to_owned()), ("Healer", "1".to_owned())]);
        assert_eq!(Request::List.fields(&auth()).len(), 3);
        assert_eq!((Request::List.path(), Request::Delete.path()), ("/party/list", "/party/del"));
        let body = String::from_utf8(form(&[("AID", "7".into())], "b")).unwrap();
        assert_eq!(body, "--b\r\nContent-Disposition: form-data; name=\"AID\"\r\n\r\n7\r\n--b--\r\n");
    }

    #[test]
    fn reads_the_answers() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{ \"Type\": 1, \"totalPage\": 1, \"data\": [{ \"AID\": 2000001, \"GID\": 150004, \"CharName\": \"LaneA1\", \"WorldName\": \"rAthena\", \"Tanker\": 0, \"Healer\": 1, \"Dealer\": 0, \"Assist\": 0, \"MinLV\": 1, \"MaxLV\": 99, \"Memo\": \"hi\", \"Type\": 2 }] }";
        let posts = posts(&response_body(response).unwrap()).unwrap();
        assert_eq!(
            posts,
            [Post {
                account: 2000001,
                character: 150004,
                name: "LaneA1".into(),
                purpose: 2,
                roles: [false, true, false, false],
                min_level: 1,
                max_level: 99,
                memo: "hi".into()
            }]
        );
        assert_eq!(super::posts("{ \"Type\": 1 }").unwrap(), []);
        assert!(super::posts("{ \"Type\": 0 }").is_err());
        assert!(response_body(b"HTTP/1.1 400 Bad Request\r\n\r\nError").is_err());
    }
}
