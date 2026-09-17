use std::{fs, path::Path, process::Command};

#[test]
fn qualified_baseline_wire_bytes_and_diagnostics_are_frozen() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let fixtures = root.join("crates/ba-cli/tests/fixtures/compatibility");
    let cases: serde_json::Value =
        serde_json::from_slice(&fs::read(fixtures.join("cases.json")).unwrap()).unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let args: Vec<_> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        let output = Command::new(env!("CARGO_BIN_EXE_ba-strategy"))
            .current_dir(&root)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code().map(i64::from),
            case["exit"].as_i64(),
            "{name}"
        );
        for (suffix, bytes) in [("stdout", output.stdout), ("stderr", output.stderr)] {
            let normalized = String::from_utf8(bytes)
                .unwrap()
                .replace(
                    &format!("\"engine_version\": \"{}\"", env!("CARGO_PKG_VERSION")),
                    "\"engine_version\": \"<PACKAGE_VERSION>\"",
                )
                .replace(root.to_str().unwrap(), "<WORKSPACE>");
            assert_eq!(
                normalized,
                fs::read_to_string(fixtures.join(format!("{name}.{suffix}"))).unwrap(),
                "{name} {suffix}"
            );
        }
    }
}
