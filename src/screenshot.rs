//! Screenshots on Print Screen: numbered JPEGs in a ScreenShot folder next to the client, named after the server.

use std::path::{Path, PathBuf};

/// The folder, under the client's own (where it runs, by its data).
pub const FOLDER: &str = "ScreenShot";

/// The first free `screen<server><NNN>.jpg` in `folder`, NNN from 000 up.
pub fn next_path(folder: &Path, server: &str) -> PathBuf {
    let server: String = server.chars().filter(|character| !matches!(character, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')).collect();
    (0..).map(|number| folder.join(format!("screen{server}{number:03}.jpg"))).find(|path| !path.exists()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_the_shots_after_those_there() {
        let folder = std::env::temp_dir().join(format!("rustnarok-shots-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(next_path(&folder, "rAthena"), folder.join("screenrAthena000.jpg"));
        std::fs::write(folder.join("screenrAthena000.jpg"), b"").unwrap();
        std::fs::write(folder.join("screenrAthena001.jpg"), b"").unwrap();
        assert_eq!(next_path(&folder, "rAthena"), folder.join("screenrAthena002.jpg"));
        // Characters a file name can't hold are left out.
        assert_eq!(next_path(&folder, "a/b:c"), folder.join("screenabc000.jpg"));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
