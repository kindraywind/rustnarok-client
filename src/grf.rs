//! Reader for GRF 0x200 archives, and the 0x300 ones ("Event Horizon") of the newer clients: one archive, or the
//! ones a client's DATA.INI lists.

mod mixcrypt;

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use flate2::read::ZlibDecoder;

const HEADER_SIZE: u64 = 46;
/// Entries without this flag are directories.
const FLAG_FILE: u8 = 0x01;

/// Undoes an encrypted archive's file data in place: the stored bytes, and the file's uncompressed size.
pub type Decrypt = Box<dyn Fn(&mut [u8], u32) + Send + Sync>;

/// The entry that tools encrypting an archive's file data add to it.
const ENCRYPTED_MARK: &str = "__encryption.info";

struct Entry {
    /// Which of the archives it's in.
    archive: usize,
    compressed_size: u32,
    aligned_size: u32,
    size: u32,
    flags: u8,
    offset: u64,
}

pub struct Grf {
    /// The archive files, the first winning where two have a file, and whether each one's data is encrypted.
    archives: Vec<(Mutex<File>, bool)>,
    entries: HashMap<String, Entry>,
    /// Loose files read before the archive (the client's data folder), by client path.
    overlay: HashMap<String, PathBuf>,
    decrypt: Option<Decrypt>,
}

impl Grf {
    /// The game data for tests that read it (`RUSTNAROK_CLIENT_GRF`, or `./data.grf`), or None, to skip them, where there's
    /// none (as on CI: game data stays out of the repository).
    pub fn for_tests() -> Option<Self> {
        let path = std::env::var("RUSTNAROK_CLIENT_GRF").unwrap_or_else(|_| "data.grf".into());
        match Self::open(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                eprintln!("skipped: no {path}");
                None
            }
            grf => Some(grf.unwrap_or_else(|error| panic!("{path}: {error}"))),
        }
    }

    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut grf = Self { archives: Vec::new(), entries: HashMap::new(), overlay: HashMap::new(), decrypt: None };
        grf.add_archive(path.as_ref())?;
        Ok(grf)
    }

    /// The archives a client's DATA.INI lists (`<n>=<file>`, beside it), the lowest number first: it wins where two
    /// have a file. An encrypted archive opens only with `decrypt`. One that doesn't open is skipped, and says why on
    /// stderr.
    pub fn open_data_ini(ini: &Path, decrypt: Option<Decrypt>) -> io::Result<Self> {
        let text = std::fs::read(ini)?;
        let mut listed: Vec<(u32, String)> = String::from_utf8_lossy(&text)
            .lines()
            .filter_map(|line| {
                let (number, name) = line.split_once('=')?;
                Some((number.trim().parse().ok()?, name.trim().to_owned()))
            })
            .collect();
        listed.sort();
        let folder = ini.parent().unwrap_or(Path::new("."));
        let mut grf = Self { archives: Vec::new(), entries: HashMap::new(), overlay: HashMap::new(), decrypt };
        for (_, name) in listed {
            if let Err(error) = grf.add_archive(&folder.join(&name)) {
                eprintln!("{name}: {error}");
            }
        }
        if grf.archives.is_empty() {
            return Err(invalid("no archive DATA.INI lists opens"));
        }
        Ok(grf)
    }

    /// Adds an archive after the ones already open: they keep the files both have.
    fn add_archive(&mut self, path: &Path) -> io::Result<()> {
        let mut file = File::open(path)?;
        let mut header = [0u8; HEADER_SIZE as usize];
        file.read_exact(&mut header)?;
        if !header.starts_with(b"Master of Magic") && !header.starts_with(b"Event Horizon") {
            return Err(invalid("not a GRF file"));
        }
        // 0x300 widens the table offset and each entry's offset to 64 bits, and puts a zero word before the table's sizes.
        let version = le32(&header[42..]);
        let (table_offset, table_pad, entry_size) = match version {
            0x200 => (u64::from(le32(&header[30..])), 0, 17),
            0x300 => (u64::from_le_bytes(header[30..38].try_into().unwrap()), 4, 21),
            _ => return Err(invalid("only GRF versions 0x200 and 0x300 are supported")),
        };

        file.seek(SeekFrom::Start(HEADER_SIZE + table_offset + table_pad))?;
        let mut sizes = [0u8; 8];
        file.read_exact(&mut sizes)?;
        let mut compressed_table = vec![0; le32(&sizes) as usize];
        file.read_exact(&mut compressed_table)?;
        let mut table = Vec::with_capacity(le32(&sizes[4..]) as usize);
        ZlibDecoder::new(&compressed_table[..]).read_to_end(&mut table)?;

        let mut entries = HashMap::new();
        let mut rest = &table[..];
        while let Some(name_end) = rest.iter().position(|&byte| byte == 0) {
            let fields = rest.get(name_end + 1..name_end + 1 + entry_size).ok_or_else(|| invalid("truncated file table"))?;
            let entry = Entry {
                compressed_size: le32(fields),
                aligned_size: le32(&fields[4..]),
                size: le32(&fields[8..]),
                flags: fields[12],
                archive: self.archives.len(),
                offset: if version == 0x300 { u64::from_le_bytes(fields[13..21].try_into().unwrap()) } else { u64::from(le32(&fields[13..])) },
            };
            if entry.flags & FLAG_FILE != 0 {
                let (name, _) = encoding_rs::EUC_KR.decode_without_bom_handling(&rest[..name_end]);
                entries.insert(normalize(&name), entry);
            }
            rest = &rest[name_end + 1 + entry_size..];
        }

        let encrypted = entries.contains_key(ENCRYPTED_MARK);
        if encrypted && self.decrypt.is_none() {
            return Err(invalid("its file data is encrypted, and nothing to decrypt it was given"));
        }
        self.archives.push((Mutex::new(file), encrypted));
        for (name, entry) in entries {
            self.entries.entry(name).or_insert(entry);
        }
        Ok(())
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

    /// Reads a loose file of the data folder, never the archive's copy: for what must come from the player's own
    /// files.
    pub fn read_loose(&self, path: &str) -> io::Result<Vec<u8>> {
        let file = self.overlay.get(&normalize(path)).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_owned()))?;
        std::fs::read(file)
    }

    /// Whether a file is there (in the archive or the data folder), by its client path.
    pub fn contains(&self, path: &str) -> bool {
        let path = normalize(path);
        self.overlay.contains_key(&path) || self.entries.contains_key(&path)
    }

    /// Reads a file by its client path, e.g. `data\texture\...`; case and slash direction don't matter.
    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        if let Some(file) = self.overlay.get(&normalize(path)) {
            return std::fs::read(file);
        }
        let entry = self.entries.get(&normalize(path)).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_owned()))?;

        let (file, encrypted) = &self.archives[entry.archive];
        let mut data = vec![0; entry.aligned_size as usize];
        {
            let mut file = file.lock().unwrap();
            file.seek(SeekFrom::Start(HEADER_SIZE + entry.offset))?;
            file.read_exact(&mut data)?;
        }
        mixcrypt::decrypt(entry.flags, entry.compressed_size, &mut data);
        if *encrypted && let Some(decrypt) = &self.decrypt {
            // Some entries of an encrypted archive are left plain: those inflate as they are.
            if let Ok(plain) = inflate(&data, entry.size)
                && plain.len() == entry.size as usize
            {
                return Ok(plain);
            }
            data.truncate(entry.compressed_size as usize);
            decrypt(&mut data, entry.size);
        }
        inflate(&data, entry.size)
    }
}

fn inflate(data: &[u8], size: u32) -> io::Result<Vec<u8>> {
    let mut decompressed = Vec::with_capacity(size as usize);
    ZlibDecoder::new(data).read_to_end(&mut decompressed)?;
    Ok(decompressed)
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
        let Some(grf) = super::Grf::for_tests() else { return };

        // flags 1: plain zlib.
        let plain = grf.read("data/texture/유저인터페이스/login_interface/bg_login.tga").unwrap();
        assert_eq!(plain.len(), 158_972);

        // flags 3: mixcrypt; a wrong decryption fails zlib or yields garbage instead of a BMP.
        let encrypted = grf.read("DATA\\texture\\유저인터페이스\\login_interface\\win_login.bmp").unwrap();
        assert_eq!(&encrypted[..2], b"BM");
        assert_eq!(encrypted.len(), 34_672);

        assert!(grf.read("data\\does\\not\\exist.bmp").is_err());
    }

    fn zlib(bytes: &[u8]) -> Vec<u8> {
        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    /// A hand-built archive of `version` 0x200 or 0x300: each file's name, stored bytes and uncompressed size.
    fn archive(version: u32, files: &[(&str, Vec<u8>, u32)]) -> Vec<u8> {
        let (mut body, mut table) = (Vec::new(), Vec::new());
        for (name, stored, size) in files {
            table.extend(name.as_bytes());
            table.push(0);
            table.extend((stored.len() as u32).to_le_bytes());
            table.extend((stored.len() as u32).to_le_bytes());
            table.extend(size.to_le_bytes());
            table.push(1);
            match version {
                0x300 => table.extend((body.len() as u64).to_le_bytes()),
                _ => table.extend((body.len() as u32).to_le_bytes()),
            }
            body.extend(stored);
        }
        let compressed_table = zlib(&table);
        let mut file = match version {
            0x300 => b"Event Horizon\0c\0".to_vec(),
            _ => b"Master of Magic\0".to_vec(),
        };
        file.extend(1u8..=14);
        match version {
            0x300 => file.extend((body.len() as u64).to_le_bytes()),
            _ => file.extend([(body.len() as u32).to_le_bytes(), 0u32.to_le_bytes()].concat()),
        }
        file.extend((files.len() as u32).to_le_bytes());
        file.extend(version.to_le_bytes());
        file.extend(&body);
        if version == 0x300 {
            file.extend(0u32.to_le_bytes());
        }
        file.extend((compressed_table.len() as u32).to_le_bytes());
        file.extend((table.len() as u32).to_le_bytes());
        file.extend(&compressed_table);
        file
    }

    fn plain(name: &str, text: &[u8]) -> (String, Vec<u8>, u32) {
        (name.to_owned(), zlib(text), text.len() as u32)
    }

    /// A folder of its own for one test's archives.
    fn folder(test: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("rustnarok-grf-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn write(folder: &std::path::Path, name: &str, version: u32, files: &[(String, Vec<u8>, u32)]) {
        let files: Vec<_> = files.iter().map(|(name, stored, size)| (name.as_str(), stored.clone(), *size)).collect();
        std::fs::write(folder.join(name), archive(version, &files)).unwrap();
    }

    /// 0x300: 64-bit offsets, the zero word before the table, "Event Horizon" for a magic.
    #[test]
    fn reads_a_0x300_archive() {
        let folder = folder("0x300");
        write(&folder, "new.grf", 0x300, &[plain("data\\sub\\hello.txt", b"hello grf")]);
        let grf = super::Grf::open(folder.join("new.grf")).unwrap();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(grf.read("DATA/sub/Hello.txt").unwrap(), b"hello grf");
    }

    /// DATA.INI's lowest number wins where two archives have a file; one that isn't there is skipped.
    #[test]
    fn opens_data_ini_in_order() {
        let folder = folder("ini");
        write(&folder, "a.grf", 0x200, &[plain("data\\both.txt", b"from a")]);
        write(&folder, "b.grf", 0x300, &[plain("data\\both.txt", b"from b"), plain("data\\only_b.txt", b"only b")]);
        std::fs::write(folder.join("DATA.INI"), "[Data]\r\n1=b.grf\r\n0=a.grf\r\n2=missing.grf\r\n").unwrap();
        let grf = super::Grf::open_data_ini(&folder.join("DATA.INI"), None).unwrap();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(grf.read("data\\both.txt").unwrap(), b"from a");
        assert_eq!(grf.read("data\\only_b.txt").unwrap(), b"only b");
    }

    /// An archive marked encrypted opens only with a decryption, which undoes the entries that don't inflate as
    /// they are.
    #[test]
    fn opens_encrypted_archives_only_with_a_decryption() {
        let folder = folder("encrypted");
        let mut secret = zlib(b"secret text");
        secret.iter_mut().for_each(|byte| *byte ^= 0x5A);
        let files = [("__encryption.info".to_owned(), vec![1, 2, 3, 4], 4), ("data\\secret.txt".to_owned(), secret, 11), plain("data\\open.txt", b"open")];
        write(&folder, "locked.grf", 0x200, &files);
        write(&folder, "base.grf", 0x200, &[plain("data\\base.txt", b"base")]);
        std::fs::write(folder.join("DATA.INI"), "[Data]\n0=locked.grf\n1=base.grf\n").unwrap();
        let ini = folder.join("DATA.INI");

        let without = super::Grf::open_data_ini(&ini, None).unwrap();
        assert!(without.read("data\\secret.txt").is_err());
        assert_eq!(without.read("data\\base.txt").unwrap(), b"base");

        let decrypt: super::Decrypt = Box::new(|data, size| {
            assert_eq!(size, 11);
            data.iter_mut().for_each(|byte| *byte ^= 0x5A);
        });
        let with = super::Grf::open_data_ini(&ini, Some(decrypt)).unwrap();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(with.read("data\\secret.txt").unwrap(), b"secret text");
        assert_eq!(with.read("data\\open.txt").unwrap(), b"open");
        assert_eq!(with.read("data\\base.txt").unwrap(), b"base");
    }

    /// Needs a 0x300 archive (the newer clients' data.grf) at `RUSTNAROK_CLIENT_GRF_0X300`.
    #[test]
    fn reads_a_real_0x300_archive() {
        let Ok(path) = std::env::var("RUSTNAROK_CLIENT_GRF_0X300") else { return };
        let grf = super::Grf::open(&path).unwrap();
        let mut paths: Vec<_> = grf.paths().collect();
        paths.sort_unstable();
        let (mut sprites, mut maps) = (0, 0);
        for path in paths.iter().step_by(paths.len() / 400) {
            let data = grf.read(path).unwrap_or_else(|error| panic!("{path}: {error}"));
            match path.rsplit('.').next() {
                Some("spr") => {
                    assert_eq!(&data[..2], b"SP");
                    sprites += 1;
                }
                Some("gat") => {
                    assert_eq!(&data[..4], b"GRAT");
                    maps += 1;
                }
                _ => {}
            }
        }
        eprintln!("{} files, read {sprites} sprites and {maps} ground files", paths.len());
        assert!(paths.len() > 200_000 && sprites > 20);
    }

    #[test]
    fn reads_windows_folder_names_back() {
        assert_eq!(super::client_name("À¯ÀúÀÎÅÍÆäÀÌ½º"), "유저인터페이스");
        assert_eq!(super::client_name("basic_interface"), "basic_interface");
        assert_eq!(super::client_name("유저인터페이스"), "유저인터페이스");
    }
}
