//! Fork-owned profile. Never redirect the process HOME or use the live Codex home.
//! Legacy relative layouts remain inside `.switcher-pet/profile`, not the real home.
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

struct RuntimeProfile { home: PathBuf, preview: bool }
static RUNTIME_PROFILE: OnceLock<RuntimeProfile> = OnceLock::new();

pub fn is_preview() -> bool {
    RUNTIME_PROFILE.get().is_some_and(|profile| profile.preview)
}

fn root_for(home: &Path) -> Result<PathBuf, String> {
    if !home.is_absolute() {
        return Err("Switcher Pet requires an absolute user home; refusing a relative fallback".into());
    }
    Ok(home.join(".switcher-pet"))
}

pub fn profile_home() -> Option<PathBuf> {
    if let Some(profile) = RUNTIME_PROFILE.get() { return Some(profile.home.clone()); }
    root_for(&dirs::home_dir()?).ok().map(|root| root.join("profile"))
}

/// Read-only exception: model metadata for the user-requested active-chat pet.
/// Callers may open rollout files for reading only; no auth/config or writes.
pub fn readonly_codex_sessions_root() -> Option<PathBuf> {
    let home=if std::env::args().any(|a|a=="--validation"){profile_home()}else{dirs::home_dir()};
    home.map(|p|p.join(".codex/sessions"))
}

/// Read-only ownership check for user-imported Google accounts. Never use this
/// path with AccountStore::load/save or in validation fixtures.
pub fn readonly_google_owner_store()->Option<PathBuf> {
    if std::env::args().any(|a|a=="--validation"){return None;}
    dirs::home_dir().map(|home|home.join(".codex-switcher/accounts.json"))
}
pub fn readonly_model_catalogs()->Vec<PathBuf> {
    let home=if std::env::args().any(|a|a=="--validation"){profile_home()}else{dirs::home_dir()};
    home.into_iter().flat_map(|h|[h.join(".codex/models_cache.codex_work_desktop.json"),h.join(".codex/models_cache.json")]).collect()
}

pub fn profile_config_dir() -> Option<PathBuf> {
    profile_home().map(|home| home.join(".config"))
}

/// Deliberately unavailable during isolated development, even with imported settings.
pub fn deny_live_integration(action: &str) -> Result<(), String> {
    Err(format!("Switcher Pet isolated mode blocks {action}; the original app and system configuration are unchanged (including macOS)"))
}

fn is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn ensure_plain_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if is_link(&metadata) || !metadata.is_dir() {
                return Err(format!("Refusing redirected or non-directory profile: {}", path.display()));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Parent is checked separately. Do not create arbitrary ancestor paths.
            fs::create_dir(path).map_err(|e| format!("Create profile {}: {e}", path.display()))?;
        }
        Err(error) => return Err(format!("Inspect profile {}: {error}", path.display())),
    }
    Ok(())
}

/// Reject existing junctions/symlinks escaping the fork. Internal skill links are OK.
/// This guards startup mistakes; it is not a sandbox against a hostile local process.
fn check_tree(path: &Path, canonical_root: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if is_link(&metadata) {
        let target = fs::canonicalize(path).map_err(|e| format!("Unresolvable profile link: {e}"))?;
        if !target.starts_with(canonical_root) {
            return Err(format!("Profile link leaves Switcher Pet: {}", path.display()));
        }
        return Ok(()); // Avoid cycles; regular directories are visited separately.
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            check_tree(&entry.map_err(|e| e.to_string())?.path(), canonical_root)?;
        }
    }
    Ok(())
}

/// Held until the event loop exits. Do not remove the lock file on release.
pub struct RuntimeGuard {
    _lock: File,
}

fn prepare_root(root: &Path) -> Result<RuntimeGuard, String> {
    ensure_plain_directory(&root)?;
    let canonical_root = fs::canonicalize(&root).map_err(|e| e.to_string())?;
    // Check before any account load, logging, migration, or child directory writes.
    check_tree(&root, &canonical_root)?;
    if root.join("import-pending.json").exists() {
        return Err("Imported configuration is paused. Resolve OAuth refresh ownership before activating this profile; the original Switcher remains unchanged.".into());
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0); // An OS-held lock, automatically released on crash.
    }
    let lock = options.open(root.join("instance.lock"))
        .map_err(|e| format!("Cannot lock Switcher Pet; another instance may be running: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Another Switcher Pet instance owns this profile".into());
        }
    }
    ensure_plain_directory(&root.join("profile"))?;
    Ok(RuntimeGuard { _lock: lock })
}

#[cfg(test)]
fn prepare_for(home: &Path) -> Result<RuntimeGuard, String> {
    prepare_root(&root_for(home)?)
}

fn prepare_selected(home: &Path, request_preview: bool) -> Result<(RuntimeGuard, RuntimeProfile), String> {
    let root = root_for(home)?;
    ensure_plain_directory(&root)?;
    let canonical = fs::canonicalize(&root).map_err(|e| e.to_string())?;
    check_tree(&root, &canonical)?;
    let preview = request_preview || root.join("import-pending.json").exists();
    // Never unpause or read the imported store. Freeze this selection for the
    // process lifetime, so removal of a marker cannot redirect a running app.
    let selected = if preview { root.join("preview") } else { root };
    let guard = prepare_root(&selected)?;
    Ok((guard, RuntimeProfile { home: selected.join("profile"), preview }))
}

pub fn prepare_runtime() -> Result<RuntimeGuard, String> {
    let home = dirs::home_dir().ok_or("Cannot locate user home; refusing to start")?;
    let requested = std::env::args_os().any(|arg| arg == "--preview");
    let (guard, profile) = if std::env::args_os().any(|arg| arg == "--validation") {
        let root = root_for(&home)?;
        ensure_plain_directory(&root)?;
        check_tree(&root, &fs::canonicalize(&root).map_err(|e| e.to_string())?)?;
        let selected = root.join("validation");
        (prepare_root(&selected)?, RuntimeProfile { home: selected.join("profile"), preview: true })
    } else { prepare_selected(&home, requested)? };
    RUNTIME_PROFILE.set(profile).map_err(|_| "Runtime profile already initialized")?;
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let home = std::env::temp_dir().join(format!("switcher-pet-paths-{}-{time}-{serial}", std::process::id()));
            fs::create_dir(&home).unwrap();
            Self(home)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn relative_home_is_rejected() {
        assert!(root_for(Path::new("relative")).is_err());
    }

    #[test]
    fn creates_only_fork_profile_and_leaves_live_data_untouched() {
        let f = Fixture::new();
        let live = f.0.join(".codex-switcher");
        fs::create_dir(&live).unwrap();
        fs::write(live.join("accounts.json"), b"do-not-touch").unwrap();
        let _guard = prepare_for(&f.0).unwrap();
        assert!(f.0.join(".switcher-pet/profile").is_dir());
        assert!(!f.0.join(".codex").exists());
        assert_eq!(fs::read(live.join("accounts.json")).unwrap(), b"do-not-touch");
    }

    #[test]
    fn second_instance_is_rejected_and_lock_recovers_after_exit() {
        let f = Fixture::new();
        let guard = prepare_for(&f.0).unwrap();
        assert!(prepare_for(&f.0).is_err());
        drop(guard);
        assert!(prepare_for(&f.0).is_ok());
    }

    #[test]
    fn regular_file_cannot_be_used_as_profile() {
        let f = Fixture::new();
        fs::write(f.0.join(".switcher-pet"), b"not-a-directory").unwrap();
        assert!(prepare_for(&f.0).is_err());
    }

    #[test]
    fn live_integrations_are_always_denied() {
        assert!(deny_live_integration("停止原版进程").is_err());
    }

    #[test]
    fn imported_profile_is_blocked_before_runtime_starts() {
        let f = Fixture::new();
        let root = root_for(&f.0).unwrap();
        fs::create_dir(&root).unwrap();
        fs::write(root.join("import-pending.json"), b"{}").unwrap();
        assert!(prepare_for(&f.0).is_err());
        assert!(!root.join("profile").exists());
        assert!(!root.join("instance.lock").exists());
    }

    #[test]
    fn pending_import_opens_separate_empty_preview_without_touching_credentials() {
        let f = Fixture::new();
        let root = root_for(&f.0).unwrap();
        fs::create_dir_all(root.join("profile/.codex-switcher")).unwrap();
        let credentials = root.join("profile/.codex-switcher/accounts.json");
        fs::write(&credentials, b"synthetic-do-not-load").unwrap();
        fs::write(root.join("import-pending.json"), b"{}").unwrap();
        let (_guard, selected) = prepare_selected(&f.0, false).unwrap();
        assert!(selected.preview);
        assert_eq!(selected.home, root.join("preview/profile"));
        assert!(!selected.home.join(".codex-switcher/accounts.json").exists());
        assert_eq!(fs::read(credentials).unwrap(), b"synthetic-do-not-load");
        assert!(root.join("import-pending.json").exists());
        assert!(!root.join("instance.lock").exists());
        assert!(prepare_selected(&f.0, false).is_err());
    }

    #[test]
    fn explicit_preview_works_without_an_import() {
        let f = Fixture::new();
        let (_guard, selected) = prepare_selected(&f.0, true).unwrap();
        assert!(selected.preview);
        assert!(!root_for(&f.0).unwrap().join("profile").exists());
    }

    #[cfg(unix)]
    #[test]
    fn escaping_link_is_rejected_without_writing_the_target() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let root = root_for(&f.0).unwrap();
        fs::create_dir(&root).unwrap();
        let target = f.0.join("live");
        fs::create_dir(&target).unwrap();
        symlink(&target, root.join("profile")).unwrap();
        assert!(prepare_for(&f.0).is_err());
        assert_eq!(fs::read_dir(target).unwrap().count(), 0);
    }
}
