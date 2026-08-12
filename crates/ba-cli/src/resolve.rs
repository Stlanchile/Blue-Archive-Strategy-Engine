use std::path::{Component, Path, PathBuf};

use ba_core::ScenarioId;

pub(crate) fn resolve_scenario(
    data_dir: &Path,
    scenario_dir: Option<&Path>,
    supplied: &Path,
) -> PathBuf {
    match scenario_dir {
        Some(directory) => resolve_with_explicit_directory(directory, supplied),
        None => resolve_legacy(data_dir, supplied),
    }
}

pub(crate) fn default_golden_directory(data_dir: &Path) -> PathBuf {
    data_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("scenarios")
        .join("golden")
}

pub(crate) fn default_example_directory(data_dir: &Path) -> PathBuf {
    data_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("scenarios")
        .join("examples")
}

fn resolve_with_explicit_directory(directory: &Path, supplied: &Path) -> PathBuf {
    if is_lexically_explicit(supplied) {
        return supplied.to_path_buf();
    }
    let mut name = supplied.as_os_str().to_os_string();
    if supplied.extension().is_none() {
        name.push(".json");
    }
    directory.join(name)
}

fn is_lexically_explicit(path: &Path) -> bool {
    if path.is_absolute() {
        return true;
    }
    let mut components = path.components();
    match components.next() {
        Some(Component::CurDir | Component::ParentDir) => true,
        Some(Component::Normal(_)) => components.next().is_some(),
        Some(Component::RootDir | Component::Prefix(_)) => true,
        None => false,
    }
}

pub(crate) fn is_bare_scenario_name(path: &Path) -> bool {
    !is_lexically_explicit(path) && matches!(path.components().next(), Some(Component::Normal(_)))
}

fn resolve_legacy(data_dir: &Path, supplied: &Path) -> PathBuf {
    if supplied.exists() || supplied.components().count() > 1 || supplied.extension().is_some() {
        return supplied.to_path_buf();
    }
    let Some(name) = supplied.to_str() else {
        return supplied.to_path_buf();
    };
    if ScenarioId::new(name).is_err() {
        return supplied.to_path_buf();
    }
    default_golden_directory(data_dir).join(format!("{name}.json"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::resolve_scenario;

    #[test]
    fn explicit_scenario_directory_classifies_original_syntax_lexically() {
        let data = Path::new("data");
        let scenarios = Path::new("selected");
        assert_eq!(
            resolve_scenario(data, Some(scenarios), Path::new("foo")),
            Path::new("selected/foo.json")
        );
        assert_eq!(
            resolve_scenario(data, Some(scenarios), Path::new("foo.json")),
            Path::new("selected/foo.json")
        );
        assert_eq!(
            resolve_scenario(data, Some(scenarios), Path::new("./foo.json")),
            Path::new("./foo.json")
        );
        assert_eq!(
            resolve_scenario(data, Some(scenarios), Path::new("dir/foo.json")),
            Path::new("dir/foo.json")
        );
        assert_eq!(
            resolve_scenario(data, Some(scenarios), Path::new("../foo.json")),
            Path::new("../foo.json")
        );
        assert_eq!(
            resolve_scenario(data, Some(scenarios), Path::new("/absolute/foo.json")),
            Path::new("/absolute/foo.json")
        );
    }
}
