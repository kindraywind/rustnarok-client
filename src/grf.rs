//! Reader for GRF 0x200 archives, the official client's data files.

mod mixcrypt;

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use flate2::read::ZlibDecoder;

const HEADER_SIZE: u64 = 46;
const ENTRY_SIZE: usize = 17;
/// Entries without this flag are directories.
const FLAG_FILE: u8 = 0x01;

struct Entry {
    compressed_size: u32,
    aligned_size: u32,
    size: u32,
    flags: u8,
    offset: u32,
}

pub struct Grf {
    file: Mutex<File>,
    entries: HashMap<String, Entry>,
    /// Loose files read before the archive (the client's data folder), by client path.
    overlay: HashMap<String, PathBuf>,
}

impl Grf {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut header = [0u8; HEADER_SIZE as usize];
        file.read_exact(&mut header)?;
        if &header[..15] != b"Master of Magic" {
            return Err(invalid("not a GRF file"));
        }
        if le32(&header[42..]) != 0x200 {
            return Err(invalid("only GRF version 0x200 is supported"));
        }

        file.seek(SeekFrom::Start(HEADER_SIZE + u64::from(le32(&header[30..]))))?;
        let mut sizes = [0u8; 8];
        file.read_exact(&mut sizes)?;
        let mut compressed_table = vec![0; le32(&sizes) as usize];
        file.read_exact(&mut compressed_table)?;
        let mut table = Vec::with_capacity(le32(&sizes[4..]) as usize);
        ZlibDecoder::new(&compressed_table[..]).read_to_end(&mut table)?;

        let mut entries = HashMap::new();
        let mut rest = &table[..];
        while let Some(name_end) = rest.iter().position(|&byte| byte == 0) {
            let fields = rest.get(name_end + 1..name_end + 1 + ENTRY_SIZE).ok_or_else(|| invalid("truncated file table"))?;
            let entry = Entry {
                compressed_size: le32(fields),
                aligned_size: le32(&fields[4..]),
                size: le32(&fields[8..]),
                flags: fields[12],
                offset: le32(&fields[13..]),
            };
            if entry.flags & FLAG_FILE != 0 {
                let (name, _) = encoding_rs::EUC_KR.decode_without_bom_handling(&rest[..name_end]);
                entries.insert(normalize(&name), entry);
            }
            rest = &rest[name_end + 1 + ENTRY_SIZE..];
        }

        Ok(Self { file: Mutex::new(file), entries, overlay: HashMap::new() })
    }

    /// Reads loose files under `root` before the archive, as the official client reads its data folder first:
    /// `root/data/...` as `data\...`, and `root/System` (or a translation's `SystemEN`) as `system\...`. Returns how
    /// many files were found.
    pub fn add_overlay(&mut self, root: &Path) -> usize {
        let before = self.overlay.len();
        for (folder, prefix) in [("data", "data"), ("System", "system"), ("SystemEN", "system")] {
            walk(&root.join(folder), prefix, &mut self.overlay);
        }
        self.overlay.len() - before
    }

    /// Every file's path, lowercase with backslashes.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().chain(self.overlay.keys().filter(|path| !self.entries.contains_key(*path))).map(String::as_str)
    }

    /// Reads a file by its client path, e.g. `data\texture\...`; case and slash direction don't matter.
    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        if let Some(file) = self.overlay.get(&normalize(path)) {
            return std::fs::read(file);
        }
        let entry = self.entries.get(&normalize(path)).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_owned()))?;

        let mut data = vec![0; entry.aligned_size as usize];
        {
            let mut file = self.file.lock().unwrap();
            file.seek(SeekFrom::Start(HEADER_SIZE + u64::from(entry.offset)))?;
            file.read_exact(&mut data)?;
        }
        mixcrypt::decrypt(entry.flags, entry.compressed_size, &mut data);

        let mut decompressed = Vec::with_capacity(entry.size as usize);
        ZlibDecoder::new(&data[..]).read_to_end(&mut decompressed)?;
        Ok(decompressed)
    }
}

/// Indexes the files under `folder` by client path (`prefix\sub\file`).
fn walk(folder: &Path, prefix: &str, index: &mut HashMap<String, PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let name = client_name(&entry.file_name().to_string_lossy());
        let path = format!("{prefix}\\{name}");
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => walk(&entry.path(), &path, index),
            Ok(_) => {
                index.insert(normalize(&path), entry.path());
            }
            Err(_) => {}
        }
    }
}

/// A file or folder name as the client knows it. Windows tools save the client's EUC-KR names as those bytes read
/// as Latin-1 (유저인터페이스 → À¯ÀúÀÎÅÍÆäÀÌ½º); this reads them back.
fn client_name(name: &str) -> String {
    if name.is_ascii() {
        return name.to_owned();
    }
    match encoding_rs::WINDOWS_1252.encode(name) {
        (bytes, _, false) => match encoding_rs::EUC_KR.decode_without_bom_handling_and_without_replacement(&bytes) {
            Some(korean) => korean.into_owned(),
            None => name.to_owned(),
        },
        _ => name.to_owned(),
    }
}

fn le32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}

fn normalize(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

#[cfg(test)]
mod tests {
    /// Needs the official data.grf, at `RUSTNAROK_CLIENT_GRF` or `./data.grf`.
    #[test]
    fn reads_plain_and_encrypted_files() {
        let grf = super::Grf::open(std::env::var("RUSTNAROK_CLIENT_GRF").unwrap_or_else(|_| "data.grf".into())).unwrap();

        // flags 1: plain zlib.
        let plain = grf.read("data/texture/유저인터페이스/login_interface/bg_login.tga").unwrap();
        assert_eq!(plain.len(), 158_972);

        // flags 3: mixcrypt; a wrong decryption fails zlib or yields garbage instead of a BMP.
        let encrypted = grf.read("DATA\\texture\\유저인터페이스\\login_interface\\win_login.bmp").unwrap();
        assert_eq!(&encrypted[..2], b"BM");
        assert_eq!(encrypted.len(), 34_672);

        assert!(grf.read("data\\does\\not\\exist.bmp").is_err());
    }

    #[test]
    fn reads_windows_folder_names_back() {
        assert_eq!(super::client_name("À¯ÀúÀÎÅÍÆäÀÌ½º"), "유저인터페이스");
        assert_eq!(super::client_name("basic_interface"), "basic_interface");
        assert_eq!(super::client_name("유저인터페이스"), "유저인터페이스");
    }
}
