#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use ba_core::{Catalog, CoreError, validate_document};
use tempfile::TempDir;

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn copy_data(destination: &Path) {
    fs::create_dir_all(destination.join("rulesets")).expect("rulesets");
    fs::create_dir_all(destination.join("rewards")).expect("rewards");
    for source in [
        "data/rulesets/jp_2026_07_29_provisional_v2.json",
        "data/rewards/jp_2026_07_29_empty_v2.json",
    ] {
        let source = workspace_path(source);
        let child = if source.to_string_lossy().contains("/rulesets/") {
            "rulesets"
        } else {
            "rewards"
        };
        fs::copy(
            &source,
            destination
                .join(child)
                .join(source.file_name().expect("name")),
        )
        .expect("copy");
    }
}

#[test]
fn selected_ambient_roots_follow_once_but_descendant_symlinks_do_not() {
    let temp = TempDir::new().expect("tempdir");
    let real = temp.path().join("real");
    copy_data(&real);
    let selected = temp.path().join("selected");
    symlink(&real, &selected).expect("ambient symlink");
    assert_eq!(
        Catalog::load(&selected)
            .expect("follow ambient root")
            .rulesets()
            .len(),
        1
    );

    let direct_parent = temp.path().join("document-parent");
    symlink(workspace_path("data/rulesets"), &direct_parent).expect("document parent");
    validate_document(
        workspace_path("data"),
        direct_parent.join("jp_2026_07_29_provisional_v2.json"),
    )
    .expect("selected document parent may resolve through a symlink");

    fs::rename(real.join("rulesets"), real.join("rulesets-real")).expect("rename child");
    symlink(real.join("rulesets-real"), real.join("rulesets")).expect("descendant symlink");
    assert!(matches!(
        Catalog::load(&selected),
        Err(CoreError::PathPolicy { .. })
    ));
}

#[test]
fn final_json_symlinks_and_json_directories_remain_rejected() {
    let temp = TempDir::new().expect("tempdir");
    copy_data(temp.path());
    symlink(
        temp.path()
            .join("rulesets/jp_2026_07_29_provisional_v2.json"),
        temp.path().join("rulesets/link.json"),
    )
    .expect("link");
    assert!(matches!(
        Catalog::load(temp.path()),
        Err(CoreError::PathPolicy { .. })
    ));
    fs::remove_file(temp.path().join("rulesets/link.json")).expect("remove");
    fs::create_dir(temp.path().join("rulesets/directory.json")).expect("directory");
    assert!(matches!(
        Catalog::load(temp.path()),
        Err(CoreError::PathPolicy { .. })
    ));
}
