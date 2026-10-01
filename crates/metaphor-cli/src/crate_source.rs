//! Fill a module's directory from the exact crate its consumer compiles, instead of a git clone.
//!
//! The `.crate` comes from Cargo's own download cache (filled by `cargo fetch --locked` in the
//! consumer when missing, so Cargo's registry, mirror and network settings apply), is checked
//! against the SHA-256 the Cargo.lock records, and is unpacked into the project's path with its
//! files read-only. The migrations and schema read from there are then byte-for-byte the release
//! that is compiled, and a local edit is caught on the next sync instead of being read silently.

use anyhow::{bail, Context, Result};
use metaphor_workspace::LockedCrate;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Written into a materialized directory; names the crate it holds.
const MARKER: &str = ".metaphor-crate";

/// Make `target_dir` hold exactly `krate`. Returns the commit the crate was published from
/// (its `.cargo_vcs_info.json`), so `metaphor.lock` keeps recording commits.
pub fn materialize(
    target_dir: &Path,
    krate: &LockedCrate,
    cargo_lock: &Path,
    update: bool,
) -> Result<String> {
    let archive = std::fs::read(cached_crate(krate, cargo_lock)?)?;
    materialize_archive(target_dir, krate, &archive, update)
}

/// [`materialize`] once the `.crate` bytes are in hand.
fn materialize_archive(
    target_dir: &Path,
    krate: &LockedCrate,
    archive: &[u8],
    update: bool,
) -> Result<String> {
    let digest = hex(&Sha256::digest(archive));
    if digest != krate.checksum {
        bail!(
            "{}-{}.crate in the cargo cache hashes to {digest}, but the Cargo.lock records {}",
            krate.name,
            krate.version,
            krate.checksum
        );
    }
    let entries = unpack_entries(archive, krate)?;

    if !update && marker_matches(target_dir, krate) {
        let changed = local_changes(target_dir, &entries)?;
        if !changed.is_empty() {
            bail!(
                "{} differs from the released {} {}: {}; it is read-only by design — run \
                 `metaphor sync --update` to restore the released files",
                target_dir.display(),
                krate.name,
                krate.version,
                changed.join(", ")
            );
        }
        return published_commit(&entries, krate);
    }

    refuse_unsaved_work(target_dir)?;
    let staging = target_dir.with_extension("metaphor-tmp");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    for (path, (bytes, executable)) in &entries {
        let dest = staging.join(path);
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::write(&dest, bytes)?;
        set_read_only(&dest, *executable)?;
    }
    std::fs::write(
        staging.join(MARKER),
        format!("{} {} {}\n", krate.name, krate.version, krate.checksum),
    )?;
    if target_dir.exists() {
        std::fs::remove_dir_all(target_dir)
            .with_context(|| format!("removing the previous {}", target_dir.display()))?;
    }
    std::fs::rename(&staging, target_dir)?;
    published_commit(&entries, krate)
}

/// Path of `<name>-<version>.crate` in Cargo's download cache, fetching it when absent.
fn cached_crate(krate: &LockedCrate, cargo_lock: &Path) -> Result<PathBuf> {
    if let Some(found) = find_in_cache(krate) {
        return Ok(found);
    }
    let manifest = cargo_lock.with_file_name("Cargo.toml");
    let status = Command::new("cargo")
        .args(["fetch", "--locked", "--manifest-path"])
        .arg(&manifest)
        .status()
        .context("spawning cargo fetch")?;
    if !status.success() {
        bail!("cargo fetch --locked failed for {}", manifest.display());
    }
    find_in_cache(krate).with_context(|| {
        format!(
            "{}-{}.crate is still not in the cargo cache after cargo fetch",
            krate.name, krate.version
        )
    })
}

fn find_in_cache(krate: &LockedCrate) -> Option<PathBuf> {
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".cargo")))?;
    let file = format!("{}-{}.crate", krate.name, krate.version);
    std::fs::read_dir(home.join("registry/cache"))
        .ok()?
        .filter_map(|e| e.ok())
        .map(|index| index.path().join(&file))
        .find(|p| p.is_file())
}

/// The archive's regular files, keyed by path with the `<name>-<version>/` prefix removed,
/// with whether each is executable.
fn unpack_entries(
    archive: &[u8],
    krate: &LockedCrate,
) -> Result<BTreeMap<PathBuf, (Vec<u8>, bool)>> {
    let prefix = format!("{}-{}", krate.name, krate.version);
    let mut entries = BTreeMap::new();
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    for entry in tar.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.into_owned();
        let Ok(rel) = path.strip_prefix(&prefix) else {
            bail!("{} holds {} outside {prefix}/", krate.name, path.display());
        };
        if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            bail!("{} holds an unsafe path {}", krate.name, path.display());
        }
        let executable = entry.header().mode().map(|m| m & 0o111 != 0).unwrap_or(false);
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        entries.insert(rel.to_path_buf(), (bytes, executable));
    }
    Ok(entries)
}

fn marker_matches(target_dir: &Path, krate: &LockedCrate) -> bool {
    std::fs::read_to_string(target_dir.join(MARKER))
        .map(|m| m.trim() == format!("{} {} {}", krate.name, krate.version, krate.checksum))
        .unwrap_or(false)
}

/// Files that were edited, removed or added since the crate was unpacked.
fn local_changes(
    target_dir: &Path,
    entries: &BTreeMap<PathBuf, (Vec<u8>, bool)>,
) -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for (path, (bytes, _)) in entries {
        match std::fs::read(target_dir.join(path)) {
            Ok(on_disk) if on_disk == *bytes => {}
            Ok(_) => changed.push(format!("{} edited", path.display())),
            Err(_) => changed.push(format!("{} removed", path.display())),
        }
    }
    for file in walkdir::WalkDir::new(target_dir).into_iter().filter_map(|e| e.ok()) {
        if !file.file_type().is_file() {
            continue;
        }
        let rel = file.path().strip_prefix(target_dir)?.to_path_buf();
        if rel != Path::new(MARKER) && !entries.contains_key(&rel) {
            changed.push(format!("{} added", rel.display()));
        }
    }
    changed.truncate(5);
    Ok(changed)
}

/// A git clone being replaced must not carry uncommitted work: that would be lost silently.
fn refuse_unsaved_work(target_dir: &Path) -> Result<()> {
    if !target_dir.join(".git").exists() {
        return Ok(());
    }
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(target_dir)
        .output()
        .context("spawning git status")?;
    if !out.status.success() || !out.stdout.is_empty() {
        bail!(
            "{} is a git clone with uncommitted changes; land or discard them before it is \
             replaced by the released crate",
            target_dir.display()
        );
    }
    Ok(())
}

fn published_commit(
    entries: &BTreeMap<PathBuf, (Vec<u8>, bool)>,
    krate: &LockedCrate,
) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct VcsInfo {
        git: Option<GitInfo>,
    }
    #[derive(serde::Deserialize)]
    struct GitInfo {
        sha1: String,
    }
    let commit = entries
        .get(Path::new(".cargo_vcs_info.json"))
        .and_then(|(bytes, _)| serde_json::from_slice::<VcsInfo>(bytes).ok())
        .and_then(|v| v.git)
        .map(|g| g.sha1);
    Ok(commit.unwrap_or_else(|| format!("sha256:{}", krate.checksum)))
}

fn set_read_only(path: &Path, executable: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o555 } else { 0o444 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(path, perms)?;
        let _ = executable;
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `.crate` as Cargo builds one: gzip'd tar, every path under `<name>-<version>/`.
    fn crate_file(name: &str, version: &str, files: &[(&str, &str)]) -> (Vec<u8>, LockedCrate) {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default()));
        for (path, body) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, format!("{name}-{version}/{path}"), body.as_bytes()).unwrap();
        }
        let bytes = tar.into_inner().unwrap().finish().unwrap();
        let krate = LockedCrate { name: name.into(), version: version.into(), checksum: hex(&Sha256::digest(&bytes)) };
        (bytes, krate)
    }

    const VCS: &str = r#"{"git":{"sha1":"0123456789abcdef0123456789abcdef01234567"},"path_in_vcs":""}"#;

    fn release(version: &str, migration: &str) -> (Vec<u8>, LockedCrate) {
        crate_file("backbone-m", version, &[(".cargo_vcs_info.json", VCS), ("migrations/1_init.up.sql", migration), ("schema/models/a.model.yaml", "name: A\n")])
    }

    #[test]
    fn unpacks_the_release_read_only_and_records_its_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("modules/backbone-m");
        let (bytes, krate) = release("1.0.0", "CREATE TABLE a ();\n");
        let commit = materialize_archive(&target, &krate, &bytes, false).unwrap();
        assert_eq!(commit, "0123456789abcdef0123456789abcdef01234567");
        let sql = target.join("migrations/1_init.up.sql");
        assert_eq!(std::fs::read_to_string(&sql).unwrap(), "CREATE TABLE a ();\n");
        assert!(std::fs::metadata(&sql).unwrap().permissions().readonly());
        assert!(target.join("schema/models/a.model.yaml").is_file());
    }

    #[test]
    fn a_crate_that_does_not_match_the_lock_checksum_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let (bytes, mut krate) = release("1.0.0", "x");
        krate.checksum = "00".repeat(32);
        let err = materialize_archive(&tmp.path().join("m"), &krate, &bytes, false).unwrap_err();
        assert!(err.to_string().contains("Cargo.lock records"), "{err:#}");
    }

    #[test]
    fn a_local_edit_is_caught_and_update_restores_the_release() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("m");
        let (bytes, krate) = release("1.0.0", "CREATE TABLE a ();\n");
        materialize_archive(&target, &krate, &bytes, false).unwrap();
        materialize_archive(&target, &krate, &bytes, false).expect("an untouched copy re-syncs cleanly");

        std::fs::write(target.join("migrations/2_stray.up.sql"), "DROP TABLE a;").unwrap();
        let err = materialize_archive(&target, &krate, &bytes, false).unwrap_err().to_string();
        assert!(err.contains("2_stray.up.sql added") && err.contains("--update"), "{err}");

        materialize_archive(&target, &krate, &bytes, true).unwrap();
        assert!(!target.join("migrations/2_stray.up.sql").exists());
        materialize_archive(&target, &krate, &bytes, false).unwrap();
    }

    #[test]
    fn a_new_release_replaces_the_previous_one_whole() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("m");
        let (v1, k1) = crate_file("backbone-m", "1.0.0", &[("migrations/old.up.sql", "a")]);
        materialize_archive(&target, &k1, &v1, false).unwrap();
        let (v2, k2) = crate_file("backbone-m", "1.1.0", &[("migrations/new.up.sql", "b")]);
        let commit = materialize_archive(&target, &k2, &v2, false).unwrap();
        assert!(target.join("migrations/new.up.sql").is_file());
        assert!(!target.join("migrations/old.up.sql").exists());
        assert_eq!(commit, format!("sha256:{}", k2.checksum), "no vcs info falls back to the checksum");
    }

    #[test]
    fn a_git_clone_is_replaced_only_when_it_holds_no_unsaved_work() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("m");
        std::fs::create_dir_all(&target).unwrap();
        let git = |args: &[&str]| {
            assert!(Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).current_dir(&target).output().unwrap().status.success());
        };
        git(&["init", "-q"]);
        std::fs::write(target.join("README"), "clone").unwrap();
        git(&["add", "README"]);
        git(&["commit", "-q", "-m", "c"]);
        std::fs::write(target.join("README"), "edited").unwrap();
        let (bytes, krate) = release("1.0.0", "x");
        let err = materialize_archive(&target, &krate, &bytes, false).unwrap_err().to_string();
        assert!(err.contains("uncommitted changes"), "{err}");
        assert_eq!(std::fs::read_to_string(target.join("README")).unwrap(), "edited", "the work is left alone");

        git(&["checkout", "-q", "--", "README"]);
        materialize_archive(&target, &krate, &bytes, false).unwrap();
        assert!(!target.join(".git").exists() && target.join("migrations/1_init.up.sql").is_file());
    }
}
