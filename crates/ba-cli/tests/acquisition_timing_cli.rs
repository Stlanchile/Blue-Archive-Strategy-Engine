use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
fn root(p: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ba-strategy"))
        .current_dir(root(""))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn timing_commands_emit_opt_in_json_and_complete_text() {
    for command in ["analyze", "simulate", "compare"] {
        let mut args = vec![command, "v3_atomic_cross_target", "--acquisition-timing"];
        if command != "analyze" {
            args.extend(["--runs", "11", "--seed", "42"]);
        }
        let text = run(&args);
        assert!(text.status.success(), "{:?}", text);
        assert!(
            String::from_utf8(text.stdout)
                .unwrap()
                .contains("Acquisition timing")
        );
        args.extend(["--format", "json"]);
        let result = run(&args);
        assert!(result.status.success(), "{:?}", result);
        assert!(result.stderr.is_empty());
        let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(value["result_schema_version"], 4);
        let legacy: Vec<_> = args
            .into_iter()
            .filter(|arg| *arg != "--acquisition-timing")
            .collect();
        let plain = run(&legacy);
        let plain: serde_json::Value = serde_json::from_slice(&plain.stdout).unwrap();
        assert_eq!(value["analysis"], plain);
    }
}
#[test]
fn profile_trace_and_validation_errors_have_empty_stdout_and_correct_precedence() {
    for command in ["analyze", "simulate", "compare"] {
        let mut args = vec![
            command,
            "single_target_200",
            "--acquisition-timing",
            "--format",
            "json",
        ];
        if command != "analyze" {
            args.extend(["--runs", "1"]);
        }
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("--acquisition-timing requires a schema-v3 scenario")
        );
        args[1] = "absent-v04-scenario.json";
        let output = run(&args);
        assert_eq!(output.status.code(), Some(4));
        assert!(output.stdout.is_empty());
    }
    let output = run(&[
        "simulate",
        "absent.json",
        "--runs",
        "1",
        "--trace",
        "--acquisition-timing",
        "--format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let output = run(&[
        "validate",
        "scenarios/golden/v3_atomic_cross_target.json",
        "--acquisition-timing",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
