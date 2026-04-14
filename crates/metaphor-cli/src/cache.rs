//! Task-result cache for multi-project plugin runs.
//!
//! Design:
//! - Cache key = blake3 hash of (plugin binary path, plugin `--version`
//!   output, argv, project file tree hash, project name).
//! - Entry stores stdout, stderr, and exit code so a hit replays the exact
//!   user-visible behavior of the original run.
//! - Only successful runs (exit 0) are cached. Failures re-run every time so
//!   a flaky test doesn't get stuck in "red" state.
//! - Cache location: `<workspace_root>/.metaphor/cache/`. Users should add
//!   `.metaphor/` to `.gitignore`.
//!
//! The format is intentionally tiny and self-describing so we can change
//! it fearlessly later — just bump `FORMAT_MAGIC` and old entries become
//! cache misses.

use anyhow::{Context, Result};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

const FORMAT_MAGIC: u32 = 0x4d43_4143; // "MCAC" — Metaphor CAChe v1
const CACHE_DIR: &str = ".metaphor/cache";

/// Hard ceiling on a single cached stdout/stderr blob. 256 MB comfortably
/// exceeds anything a lint/test plugin will produce; reject larger to guard
/// against corrupted cache files claiming absurd lengths.
const MAX_BLOB_BYTES: u64 = 256 * 1024 * 1024;

/// Directories ignored when hashing a project tree. These are either
/// generated output (target, dist) or tool state (.git, node_modules).
/// Keep this conservative — if anything important is here, we'd produce
/// false cache hits.
const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".metaphor",
    "target",
    "node_modules",
    "dist",
    "build",
    ".next",
    ".venv",
    "__pycache__",
];

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CacheKey(pub [u8; 32]);

impl CacheKey {
    pub fn to_hex(&self) -> String {
        blake3::Hash::from(self.0).to_hex().to_string()
    }
}

/// Inputs that together define a cacheable task invocation.
#[derive(Clone, Copy)]
pub struct KeyInputs<'a> {
    pub plugin_binary: &'a Path,
    pub plugin_version: &'a str,
    pub argv: &'a [String],
    pub project_name: &'a str,
    pub project_tree_hash: [u8; 32],
}

impl<'a> KeyInputs<'a> {
    pub fn compute_key(&self) -> CacheKey {
        let mut h = blake3::Hasher::new();
        h.update(b"metaphor-cache-v1\0");
        h.update(self.plugin_binary.to_string_lossy().as_bytes());
        h.update(b"\0");
        h.update(self.plugin_version.as_bytes());
        h.update(b"\0");
        for a in self.argv {
            h.update(a.as_bytes());
            h.update(b"\0");
        }
        h.update(b"\0"); // separate argv from project name
        h.update(self.project_name.as_bytes());
        h.update(b"\0");
        h.update(&self.project_tree_hash);
        CacheKey(h.finalize().into())
    }
}

#[derive(Clone)]
pub struct CacheEntry {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn open(workspace_root: &Path) -> Result<Self> {
        let root = workspace_root.join(CACHE_DIR);
        fs::create_dir_all(&root)
            .with_context(|| format!("creating cache dir {}", root.display()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn entry_path(&self, key: CacheKey) -> PathBuf {
        self.root.join(format!("{}.bin", key.to_hex()))
    }

    pub fn get(&self, key: CacheKey) -> Option<CacheEntry> {
        let path = self.entry_path(key);
        let mut f = fs::File::open(&path).ok()?;
        decode_entry(&mut f).ok()
    }

    /// Only call for successful runs. Writing failures poisons the cache.
    pub fn put(&self, key: CacheKey, entry: &CacheEntry) -> Result<()> {
        let path = self.entry_path(key);
        // Atomic-ish write via rename.
        let tmp = path.with_extension("tmp");
        {
            let mut f =
                fs::File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
            encode_entry(&mut f, entry)?;
        }
        fs::rename(&tmp, &path)
            .with_context(|| format!("finalizing cache entry {}", path.display()))?;
        Ok(())
    }

    pub fn clear(&self) -> Result<ClearStats> {
        let mut removed = 0usize;
        let mut bytes = 0u64;
        if !self.root.exists() {
            return Ok(ClearStats { removed, bytes });
        }
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                bytes += entry.metadata()?.len();
                fs::remove_file(&path)?;
                removed += 1;
            }
        }
        Ok(ClearStats { removed, bytes })
    }

    pub fn stats(&self) -> Result<CacheStats> {
        let mut entries = 0usize;
        let mut bytes = 0u64;
        let mut newest: Option<SystemTime> = None;
        if self.root.exists() {
            for entry in fs::read_dir(&self.root)? {
                let entry = entry?;
                if !entry.path().is_file() {
                    continue;
                }
                let md = entry.metadata()?;
                if md.len() == 0 {
                    continue; // skip in-flight .tmp files
                }
                entries += 1;
                bytes += md.len();
                if let Ok(modified) = md.modified() {
                    newest = Some(newest.map_or(modified, |n| n.max(modified)));
                }
            }
        }
        Ok(CacheStats {
            root: self.root.clone(),
            entries,
            bytes,
            newest,
        })
    }
}

pub struct ClearStats {
    pub removed: usize,
    pub bytes: u64,
}

pub struct CacheStats {
    pub root: PathBuf,
    pub entries: usize,
    pub bytes: u64,
    pub newest: Option<SystemTime>,
}

fn encode_entry<W: Write>(w: &mut W, entry: &CacheEntry) -> Result<()> {
    w.write_all(&FORMAT_MAGIC.to_le_bytes())?;
    w.write_all(&entry.exit_code.to_le_bytes())?;
    write_blob(w, &entry.stdout)?;
    write_blob(w, &entry.stderr)?;
    Ok(())
}

fn decode_entry<R: Read>(r: &mut R) -> Result<CacheEntry> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    if u32::from_le_bytes(magic) != FORMAT_MAGIC {
        anyhow::bail!("bad cache format (old entry?)");
    }
    let mut exit = [0u8; 4];
    r.read_exact(&mut exit)?;
    let stdout = read_blob(r)?;
    let stderr = read_blob(r)?;
    Ok(CacheEntry {
        exit_code: i32::from_le_bytes(exit),
        stdout,
        stderr,
    })
}

fn write_blob<W: Write>(w: &mut W, data: &[u8]) -> Result<()> {
    w.write_all(&(data.len() as u64).to_le_bytes())?;
    w.write_all(data)?;
    Ok(())
}

fn read_blob<R: Read>(r: &mut R) -> Result<Vec<u8>> {
    let mut len = [0u8; 8];
    r.read_exact(&mut len)?;
    let len = u64::from_le_bytes(len);
    if len > MAX_BLOB_BYTES {
        anyhow::bail!(
            "cache blob too large: {} bytes (max {})",
            len,
            MAX_BLOB_BYTES
        );
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

/// Hash every tracked-ish file under `project_root`. Returns a stable 32-byte
/// digest that changes iff any file's relative path or contents change.
///
/// Files are streamed — a stray large binary in the tree won't blow up memory.
pub fn hash_project_tree(project_root: &Path) -> Result<[u8; 32]> {
    if !project_root.exists() {
        anyhow::bail!("project path does not exist: {}", project_root.display());
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(project_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_ignored_dir(e.file_name().to_string_lossy().as_ref()))
    {
        let entry = entry
            .with_context(|| format!("walking {} for cache hashing", project_root.display()))?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    // Stable, deterministic order.
    files.sort_unstable();

    let mut h = blake3::Hasher::new();
    h.update(b"metaphor-tree-v1\0");
    for path in &files {
        let rel = path.strip_prefix(project_root).unwrap_or(path);
        h.update(rel.to_string_lossy().as_bytes());
        h.update(b"\0");
        // Include file length as a length-prefix so `cat a b` and `cat ab`
        // hash differently even before contents are mixed in.
        let len = fs::metadata(path)
            .with_context(|| format!("stat {}", path.display()))?
            .len();
        h.update(&len.to_le_bytes());
        let mut f = fs::File::open(path).with_context(|| format!("reading {}", path.display()))?;
        h.update_reader(&mut f)
            .with_context(|| format!("hashing {}", path.display()))?;
    }
    Ok(h.finalize().into())
}

fn is_ignored_dir(name: &str) -> bool {
    IGNORED_DIRS.iter().any(|d| *d == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn key_changes_with_any_input() {
        let base = KeyInputs {
            plugin_binary: Path::new("/bin/p"),
            plugin_version: "1.0",
            argv: &["lint".into(), "--strict".into()],
            project_name: "api",
            project_tree_hash: [0u8; 32],
        };
        let k0 = base.compute_key();

        let mut mutated = base;
        mutated.plugin_version = "1.1";
        assert_ne!(k0, mutated.compute_key());

        let mut mutated = base;
        let new_argv: Vec<String> = vec!["lint".into()];
        mutated.argv = &new_argv;
        assert_ne!(k0, mutated.compute_key());

        let mut mutated = base;
        mutated.project_name = "web";
        assert_ne!(k0, mutated.compute_key());

        let mut mutated = base;
        let mut new_tree = [0u8; 32];
        new_tree[0] = 1;
        mutated.project_tree_hash = new_tree;
        assert_ne!(k0, mutated.compute_key());
    }

    #[test]
    fn round_trip_entry() {
        let tmp = TempDir::new().unwrap();
        let cache = Cache::open(tmp.path()).unwrap();
        let key = CacheKey([7u8; 32]);
        let entry = CacheEntry {
            exit_code: 0,
            stdout: b"hello\n".to_vec(),
            stderr: b"warn\n".to_vec(),
        };
        cache.put(key, &entry).unwrap();
        let got = cache.get(key).unwrap();
        assert_eq!(got.exit_code, 0);
        assert_eq!(got.stdout, entry.stdout);
        assert_eq!(got.stderr, entry.stderr);
    }

    #[test]
    fn tree_hash_changes_on_file_edit() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path();
        fs::write(p.join("a.txt"), "hello").unwrap();
        let h1 = hash_project_tree(p).unwrap();
        fs::write(p.join("a.txt"), "HELLO").unwrap();
        let h2 = hash_project_tree(p).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn tree_hash_ignores_target_dir() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path();
        fs::write(p.join("src.rs"), "x").unwrap();
        let h1 = hash_project_tree(p).unwrap();
        fs::create_dir_all(p.join("target/debug")).unwrap();
        fs::write(p.join("target/debug/out"), "LOTS OF BYTES").unwrap();
        let h2 = hash_project_tree(p).unwrap();
        assert_eq!(h1, h2, "target/ must not affect the tree hash");
    }

    #[test]
    fn clear_removes_entries() {
        let tmp = TempDir::new().unwrap();
        let cache = Cache::open(tmp.path()).unwrap();
        cache
            .put(
                CacheKey([1u8; 32]),
                &CacheEntry {
                    exit_code: 0,
                    stdout: b"x".to_vec(),
                    stderr: Vec::new(),
                },
            )
            .unwrap();
        let stats = cache.stats().unwrap();
        assert_eq!(stats.entries, 1);
        let cleared = cache.clear().unwrap();
        assert_eq!(cleared.removed, 1);
        assert_eq!(cache.stats().unwrap().entries, 0);
    }
}
