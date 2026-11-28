//! Reads the image config out of a `docker save` tar. The reader seeks past every member it does not
//! need, holds at most one 1 MiB member in memory, and judges nothing: the policy on user, entrypoint
//! and platform is applied by `publish`. Every failure is the fixed text `archive unreadable`.

use serde::Deserialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const BAD: &str = "archive unreadable";
/// Largest member read into memory; a member of exactly this size passes.
const MEMBER_MAX: u64 = 1 << 20;

#[derive(Debug)]
pub struct ImageConfig {
    pub os: String,
    pub architecture: String,
    pub user: String,
    pub entrypoint: Vec<String>,
    pub cmd: Vec<String>,
}

#[derive(Deserialize)]
struct ManifestEntry {
    #[serde(rename = "Config")]
    config: String,
}

#[derive(Deserialize)]
struct Config {
    os: Option<String>,
    architecture: Option<String>,
    config: Option<Inner>,
}

#[derive(Deserialize)]
struct Inner {
    #[serde(rename = "User")]
    user: Option<String>,
    #[serde(rename = "Entrypoint")]
    entrypoint: Option<Vec<String>>,
    #[serde(rename = "Cmd")]
    cmd: Option<Vec<String>>,
}

fn octal(field: &[u8]) -> Option<u64> {
    let text = std::str::from_utf8(field).ok()?;
    u64::from_str_radix(text.trim_matches(|c| c == '\0' || c == ' '), 8).ok()
}

/// Reads the data of the first regular member called `name`, scanning from the start of the file.
fn member(path: &Path, name: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut f = File::open(path).map_err(|_| BAD)?;
    let len = f.metadata().map_err(|_| BAD)?.len();
    let mut pos = 0u64;
    loop {
        let mut h = [0u8; 512];
        // A short read or a zero block ends the archive; the caller then sees a missing member.
        if f.read_exact(&mut h).is_err() || h.iter().all(|b| *b == 0) {
            return Err(BAD);
        }
        let mut sum = 0u64;
        for (i, b) in h.iter().enumerate() {
            sum += if (148..156).contains(&i) { 32 } else { *b as u64 };
        }
        if octal(&h[148..156]) != Some(sum) {
            return Err(BAD);
        }
        let size = octal(&h[124..136]).ok_or(BAD)?;
        let data = pos + 512;
        if data + size > len {
            return Err(BAD);
        }
        let end = h[..100].iter().position(|b| *b == 0).unwrap_or(100);
        if matches!(h[156], b'0' | 0) && &h[..end] == name {
            if size > MEMBER_MAX {
                return Err(BAD);
            }
            let mut buf = vec![0u8; size as usize];
            f.read_exact(&mut buf).map_err(|_| BAD)?;
            return Ok(buf);
        }
        pos = data + size.div_ceil(512) * 512;
        f.seek(SeekFrom::Start(pos)).map_err(|_| BAD)?;
    }
}

pub fn read_config(path: &Path) -> Result<ImageConfig, &'static str> {
    let manifest: Vec<ManifestEntry> =
        serde_json::from_slice(&member(path, b"manifest.json")?).map_err(|_| BAD)?;
    let [entry] = manifest.as_slice() else {
        return Err(BAD);
    };
    let c: Config = serde_json::from_slice(&member(path, entry.config.as_bytes())?).map_err(|_| BAD)?;
    let i = c.config;
    Ok(ImageConfig {
        os: c.os.unwrap_or_default(),
        architecture: c.architecture.unwrap_or_default(),
        user: i.as_ref().and_then(|i| i.user.clone()).unwrap_or_default(),
        entrypoint: i.as_ref().and_then(|i| i.entrypoint.clone()).unwrap_or_default(),
        cmd: i.and_then(|i| i.cmd).unwrap_or_default(),
    })
}
