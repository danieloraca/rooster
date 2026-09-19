use rooster_core::{ConfigStore, NativePath};
use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};
use tempfile::tempdir;

#[test]
fn registration_relocation_and_read_only_defaults() {
    let temp = tempdir().unwrap();
    let settings = temp.path().join("settings/config.json");
    let store = ConfigStore::new(&settings).unwrap();
    assert!(store.load().unwrap().workspaces.is_empty());
    assert!(!settings.parent().unwrap().exists());
    let first = temp.path().join("repos with spaces — 東京");
    let second = temp.path().join("other");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let registration = store.add("Personal", &first).unwrap();
    let same = store.add(" Personal ", &first).unwrap();
    assert_eq!(same.root.id, registration.root.id);
    let other = store.add("Personal", &second).unwrap();
    assert_eq!(other.workspace_id, registration.workspace_id);
    assert_ne!(other.root.id, registration.root.id);
    let shared = store.add("Team", &first).unwrap();
    assert_ne!(shared.root.id, registration.root.id);
    assert_eq!(
        store.load().unwrap().roots(Some("Personal")).unwrap().len(),
        2
    );
    assert_eq!(
        store
            .load()
            .unwrap()
            .roots(Some(&shared.workspace_id))
            .unwrap()
            .len(),
        1
    );
    assert!(store.load().unwrap().roots(Some("absent")).is_err());
    let relocated = temp.path().join("moved");
    fs::rename(&first, &relocated).unwrap();
    let moved = store.relocate(&registration.root.id, &relocated).unwrap();
    assert_eq!(moved.root.id, registration.root.id);
    assert_eq!(moved.root.path.as_path(), relocated.canonicalize().unwrap());
    let before = fs::read(&settings).unwrap();
    assert!(store.relocate(&registration.root.id, &second).is_err());
    assert!(store.relocate("unknown", &second).is_err());
    assert!(store.add("", &second).is_err());
    assert!(store.add("Missing", temp.path().join("missing")).is_err());
    assert_eq!(fs::read(&settings).unwrap(), before);
    assert_eq!(store.load().unwrap().schema_version, 1);
}

#[test]
fn unsupported_or_malformed_settings_are_preserved() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.json");
    let store = ConfigStore::new(&path).unwrap();
    for content in [
        "{broken",
        "{\"schema_version\":99,\"next_id\":1,\"workspaces\":[],\"excluded_dirs\":[]}",
        "{\"schema_version\":1,\"next_id\":1,\"workspaces\":[],\"excluded_dirs\":[],\"future\":true}",
        "{\"schema_version\":1,\"next_id\":1,\"workspaces\":[],\"excluded_dirs\":[\"../secret\"]}",
    ] {
        fs::write(&path, content).unwrap();
        assert!(store.load().is_err());
        assert!(store.add("Test", temp.path()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
    }
}

#[test]
fn concurrent_writers_reload_under_the_lock() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.json");
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|index| {
            let root = temp.path().join(format!("root-{index}"));
            fs::create_dir(&root).unwrap();
            let store = ConfigStore::new(&path).unwrap();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                store.add("Team", root).unwrap();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let config = ConfigStore::new(path).unwrap().load().unwrap();
    let roots = config.roots(Some("Team")).unwrap();
    assert_eq!(roots.len(), 8);
    let ids: std::collections::HashSet<_> = roots.iter().map(|r| &r.id).collect();
    assert_eq!(ids.len(), 8);
}

#[test]
fn unicode_paths_serialize_as_readable_strings() {
    let path = NativePath::from(std::path::PathBuf::from("repos — 東京"));
    let encoded = serde_json::to_string(&path).unwrap();
    assert_eq!(encoded, "\"repos — 東京\"");
    assert_eq!(serde_json::from_str::<NativePath>(&encoded).unwrap(), path);
}

#[cfg(unix)]
#[test]
fn native_paths_roundtrip_and_settings_symlinks_are_rejected() {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let temp = tempdir().unwrap();
    let odd = temp
        .path()
        .join(std::ffi::OsString::from_vec(b"repo-\xff".to_vec()));
    // APFS rejects invalid UTF-8 filenames. Test the lossless representation
    // independently of filesystem support, then use a valid path for I/O.
    let native = NativePath::from(odd);
    let encoded = serde_json::to_string(&native).unwrap();
    assert!(encoded.contains("unix_bytes"));
    assert_eq!(
        serde_json::from_str::<NativePath>(&encoded).unwrap(),
        native
    );
    let odd = temp.path().join("repo — café");
    fs::create_dir(&odd).unwrap();
    let path = temp.path().join("config.json");
    let store = ConfigStore::new(&path).unwrap();
    store.add("Native", &odd).unwrap();
    assert_eq!(
        store.load().unwrap().roots(None).unwrap()[0].path.as_path(),
        odd.canonicalize().unwrap()
    );
    let alias = temp.path().join("alias.json");
    symlink(&path, &alias).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(
        ConfigStore::new(alias)
            .unwrap()
            .add("Test", temp.path())
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn removal_forgets_only_the_selected_registration_and_never_touches_sources() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.json");
    let store = ConfigStore::new(&path).unwrap();
    let parent = temp.path().join("repos");
    let child = parent.join("Project");
    fs::create_dir_all(&child).unwrap();
    let guidance = child.join("AGENTS.md");
    fs::write(&guidance, "Keep this source unchanged.\n").unwrap();
    let first = store.add("Work", &parent).unwrap();
    let nested = store.add("Work", &child).unwrap();
    let shared = store.add("Other", &parent).unwrap();
    let before = store.load().unwrap();

    store.remove_root(&first.root.id).unwrap();
    let remaining = store.load().unwrap();
    assert_eq!(remaining.workspaces.len(), 2);
    assert_eq!(
        remaining.roots(Some("Work")).unwrap(),
        vec![nested.root.clone()]
    );
    assert_eq!(remaining.roots(Some("Other")).unwrap(), vec![shared.root]);
    assert_eq!(remaining.codex, before.codex);
    assert_eq!(remaining.claude, before.claude);
    assert_eq!(remaining.excluded_dirs, before.excluded_dirs);
    assert_eq!(
        fs::read_to_string(&guidance).unwrap(),
        "Keep this source unchanged.\n"
    );

    let bytes = fs::read(&path).unwrap();
    assert!(matches!(
        store.remove_root(&first.root.id),
        Err(rooster_core::Error::RootNotFound(_))
    ));
    assert!(store.remove_root("../../not-a-root").is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);

    // No canonicalization or existence check is needed to forget an offline root.
    fs::rename(&parent, temp.path().join("moved repos")).unwrap();
    store.remove_root(&nested.root.id).unwrap();
    let remaining = store.load().unwrap();
    assert_eq!(remaining.workspaces.len(), 1);
    assert_eq!(remaining.workspaces[0].name, "Other");
    assert_eq!(
        fs::read_to_string(temp.path().join("moved repos/Project/AGENTS.md")).unwrap(),
        "Keep this source unchanged.\n"
    );
    let added = store
        .add("Work", temp.path().join("moved repos/Project"))
        .unwrap();
    assert_ne!(added.workspace_id, first.workspace_id);
    assert_ne!(added.root.id, nested.root.id);
}
