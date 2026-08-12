#![forbid(unsafe_code)]

mod args;
mod command;
mod errors;
mod render;
mod resolve;

use std::ffi::OsString;
use std::io::Write;

use args::{Cli, OutputFormat, requests_json};
use clap::Parser;
use clap::error::ErrorKind;
use errors::{DiagnosticEnvelope, ErrorEnvelope, classify_error};

pub fn run<I, T>(args: I, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let raw = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let usage_json = requests_json(&raw);
    let cli = match Cli::try_parse_from(raw) {
        Ok(value) => value,
        Err(error) => {
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                let rendered = error.render().to_string();
                return if stdout.write_all(rendered.as_bytes()).is_ok() {
                    0
                } else {
                    let _ = stderr.write_all(b"unexpected failure: could not write stdout\n");
                    70
                };
            }
            let classified = errors::usage_error(error.to_string());
            let rendered = if usage_json {
                render::render_json(&ErrorEnvelope {
                    error: classified.body,
                })
                .unwrap_or_else(|_| {
                    "{\"error\":{\"class\":\"cli_usage\",\"code\":\"cli_usage\",\"message\":\"invalid command line\"}}\n".to_owned()
                })
            } else {
                error.render().to_string()
            };
            let _ = stderr.write_all(rendered.as_bytes());
            return 2;
        }
    };

    let (mode, result) = command::execute(cli);
    match result {
        Ok(rendered) => {
            if stdout.write_all(rendered.as_bytes()).is_ok() {
                0
            } else {
                let _ = stderr.write_all(b"unexpected failure: could not write stdout\n");
                70
            }
        }
        Err(error) => {
            let classified = classify_error(error);
            let rendered = if mode.diagnostics {
                render::render_json(&DiagnosticEnvelope {
                    diagnostics_schema_version: 1,
                    error: classified.diagnostic,
                })
                .unwrap_or_else(|_| {
                    "{\"diagnostics_schema_version\":1,\"error\":{\"class\":\"internal\",\"code\":\"render_failure\",\"message\":\"could not render diagnostics\"}}\n".to_owned()
                })
            } else {
                match mode.format {
                    OutputFormat::Json => render::render_json(&ErrorEnvelope {
                        error: classified.body,
                    })
                    .unwrap_or_else(|_| {
                        "{\"error\":{\"class\":\"internal\",\"code\":\"render_failure\",\"message\":\"could not render error\"}}\n".to_owned()
                    }),
                    OutputFormat::Text => format!(
                        "error [{}:{}]: {}\n",
                        classified.body.class,
                        classified.body.code,
                        classified.body.message
                    ),
                }
            };
            let _ = stderr.write_all(rendered.as_bytes());
            classified.exit
        }
    }
}

#[cfg(test)]
mod tests {
    use super::command::resolve_master_seed_with;
    use super::errors::AppError;

    #[test]
    fn explicit_seed_does_not_consult_entropy() {
        let seed =
            resolve_master_seed_with(Some(42), || Err("entropy callback must not run".to_owned()))
                .expect("explicit seed");
        assert_eq!(seed, 42);
    }

    #[test]
    fn entropy_failure_is_fail_closed() {
        assert!(matches!(
            resolve_master_seed_with(None, || Err("unavailable".to_owned())),
            Err(AppError::Entropy(message)) if message == "unavailable"
        ));
    }
}
