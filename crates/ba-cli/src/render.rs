use std::fmt::Write as _;

use ba_core::ValidationReport;
use ba_engine::{
    ComparisonResult, ComparisonResultV3, ExactAnalysisResult, ExactAnalysisResultV3,
    MonteCarloAnalysisResult, MonteCarloAnalysisResultV3, RunTraceResult, RunTraceResultV3,
};
use serde::Serialize;

use crate::args::OutputFormat;
use crate::errors::AppError;

pub(crate) fn validation(
    value: &ValidationReport,
    format: OutputFormat,
) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Valid: yes\nDocument type: {}\nSchema version: {}\nID: {}\nBehavior fingerprint: {}\nDocument fingerprint: {}\n",
            value.document_type,
            value.schema_version,
            value.id,
            value.behavior_fingerprint,
            value.document_fingerprint,
        )),
    }
}

pub(crate) fn exact(value: &ExactAnalysisResult, format: OutputFormat) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: exact\nScenario: {}\nSuccess probability: {:.15}\nExpected terminal primitive recruitments: {:.10}\nExpected first-success recruitment count given success: {}\nExpected paid pyroxene spent: {:.10}\nExpected ticket-funded primitive recruitments: {:.10}\n",
            value.provenance.scenario_id,
            value.success_probability,
            value.expected_terminal_primitive_recruitments,
            display_optional(value.expected_first_success_recruitment_count_given_success),
            value.expected_paid_pyroxene_spent,
            value.expected_ticket_funded_primitive_recruitments,
        )),
    }
}

pub(crate) fn monte_carlo(
    value: &MonteCarloAnalysisResult,
    format: OutputFormat,
) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: Monte Carlo\nScenario: {}\nRuns: {}\nMaster seed: {}\nSuccess probability estimate: {:.15}\nExpected terminal primitive recruitments: {:.10}\nExpected paid pyroxene spent: {:.10}\nExpected ticket-funded primitive recruitments: {:.10}\n",
            value.provenance.scenario_id,
            value.rng.run_count,
            value.rng.master_seed,
            value.success_probability,
            value.expected_terminal_primitive_recruitments,
            value.expected_paid_pyroxene_spent,
            value.expected_ticket_funded_primitive_recruitments,
        )),
    }
}

pub(crate) fn trace(value: &RunTraceResult, format: OutputFormat) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: trace\nScenario: {}\nMaster seed: {}\nTerminal reason: {:?}\nTerminal primitive recruitments: {}\nFirst-success recruitment count: {}\nPaid pyroxene spent: {}\nTicket-funded primitive recruitments: {}\n",
            value.provenance.scenario_id,
            value
                .rng
                .as_ref()
                .map_or_else(|| "none".to_owned(), |rng| rng.master_seed.to_string()),
            value.terminal_reason,
            value.terminal_primitive_recruitments,
            value
                .first_success_recruitment_count
                .map_or_else(|| "none".to_owned(), |count| count.to_string()),
            value.paid_pyroxene_spent,
            value.ticket_funded_primitive_recruitments,
        )),
    }
}

pub(crate) fn comparison(
    value: &ComparisonResult,
    format: OutputFormat,
) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: comparison\nScenario: {}\nMaster seed: {}\nExact success probability: {:.15}\nMonte Carlo success probability: {:.15}\nDifference: {:.15}\nExact value inside Monte Carlo 95% interval: {}\n",
            value.exact.provenance.scenario_id,
            value.monte_carlo.rng.master_seed,
            value.exact.success_probability,
            value.monte_carlo.success_probability,
            value.success_probability_difference,
            if value.success_probability_within_monte_carlo_interval {
                "yes"
            } else {
                "no"
            },
        )),
    }
}

pub(crate) fn exact_v3(
    value: &ExactAnalysisResultV3,
    format: OutputFormat,
) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: exact\nScenario: {}\nAll-target success probability: {:.15}\nExpected additional primitive recruitments: {:.10}\nExpected first all-target completion count given success: {}\nExpected paid pyroxene spent: {:.10}\nExpected ticket-funded primitive recruitments: {:.10}\n",
            value.provenance.scenario_id,
            value.all_target_success_probability,
            value.expected_additional_primitive_recruitments,
            display_optional(value.expected_first_all_target_completion_count_given_success),
            value.expected_paid_pyroxene_spent,
            value.expected_ticket_funded_primitive_recruitments,
        )),
    }
}

pub(crate) fn monte_carlo_v3(
    value: &MonteCarloAnalysisResultV3,
    format: OutputFormat,
) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: Monte Carlo\nScenario: {}\nRuns: {}\nMaster seed: {}\nAll-target success probability estimate: {:.15}\nExpected additional primitive recruitments: {:.10}\nExpected paid pyroxene spent: {:.10}\nExpected ticket-funded primitive recruitments: {:.10}\n",
            value.provenance.scenario_id,
            value.rng.run_count,
            value.rng.master_seed,
            value.all_target_success_probability,
            value.expected_additional_primitive_recruitments,
            value.expected_paid_pyroxene_spent,
            value.expected_ticket_funded_primitive_recruitments,
        )),
    }
}

pub(crate) fn trace_v3(value: &RunTraceResultV3, format: OutputFormat) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: trace\nScenario: {}\nMaster seed: {}\nTerminal reason: {:?}\nTerminal additional primitive recruitments: {}\nTerminal absolute campaign recruitment count: {}\nFirst all-target completion additional count: {}\nPaid pyroxene spent: {}\nTicket-funded primitive recruitments: {}\n",
            value.provenance.scenario_id,
            value
                .rng
                .as_ref()
                .map_or_else(|| "none".to_owned(), |rng| rng.master_seed.to_string()),
            value.terminal_reason,
            value.terminal_additional_primitive_recruitments,
            value.terminal_absolute_campaign_recruitment_count,
            value
                .first_all_target_completion_additional_count
                .map_or_else(|| "none".to_owned(), |count| count.to_string()),
            value.paid_pyroxene_spent,
            value.ticket_funded_primitive_recruitments,
        )),
    }
}

pub(crate) fn comparison_v3(
    value: &ComparisonResultV3,
    format: OutputFormat,
) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: comparison\nScenario: {}\nMaster seed: {}\nExact all-target success probability: {:.15}\nMonte Carlo all-target success probability: {:.15}\nDifference: {:.15}\nExact value inside Monte Carlo 95% interval: {}\n",
            value.exact.provenance.scenario_id,
            value.monte_carlo.rng.master_seed,
            value.exact.all_target_success_probability,
            value.monte_carlo.all_target_success_probability,
            value.all_target_success.simulation_minus_exact,
            if value.all_target_success.exact_within_monte_carlo_interval {
                "yes"
            } else {
                "no"
            },
        )),
    }
}

pub(crate) fn structured(value: &impl Serialize, format: OutputFormat) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => {
            let value = serde_json::to_value(value).map_err(|error| {
                AppError::Internal(format!("structured text conversion failed: {error}"))
            })?;
            let mut rendered = String::new();
            render_text_node(&value, 0, &mut rendered);
            Ok(rendered)
        }
    }
}

pub(crate) fn render_json(value: &impl Serialize) -> Result<String, AppError> {
    let mut rendered = serde_json::to_string_pretty(value)
        .map_err(|error| AppError::Internal(format!("JSON rendering failed: {error}")))?;
    rendered.push('\n');
    Ok(rendered)
}

pub(crate) fn terminal_safe(value: &str) -> String {
    let mut rendered = String::with_capacity(value.len());
    push_terminal_safe(&mut rendered, value, false);
    rendered
}

pub(crate) fn terminal_safe_multiline(value: &str) -> String {
    let mut rendered = String::with_capacity(value.len());
    push_terminal_safe(&mut rendered, value, true);
    rendered
}

fn display_optional(value: Option<f64>) -> String {
    value.map_or_else(|| "null".to_owned(), |number| format!("{number:.10}"))
}

fn render_text_node(value: &serde_json::Value, indentation: usize, output: &mut String) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                push_indentation(output, indentation);
                output.push_str(key);
                output.push(':');
                if is_scalar(value) {
                    output.push(' ');
                    push_scalar(output, value);
                    output.push('\n');
                } else {
                    output.push('\n');
                    render_text_node(value, indentation + 2, output);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                push_indentation(output, indentation);
                output.push('-');
                if is_scalar(value) {
                    output.push(' ');
                    push_scalar(output, value);
                    output.push('\n');
                } else {
                    output.push('\n');
                    render_text_node(value, indentation + 2, output);
                }
            }
        }
        scalar => {
            push_indentation(output, indentation);
            push_scalar(output, scalar);
            output.push('\n');
        }
    }
}

fn is_scalar(value: &serde_json::Value) -> bool {
    !matches!(
        value,
        serde_json::Value::Array(_) | serde_json::Value::Object(_)
    )
}

fn push_scalar(output: &mut String, value: &serde_json::Value) {
    match value {
        serde_json::Value::Null => output.push_str("null"),
        serde_json::Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        serde_json::Value::Number(value) => output.push_str(&value.to_string()),
        serde_json::Value::String(value) => push_terminal_safe(output, value, false),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            unreachable!("structured values are rendered recursively")
        }
    }
}

fn push_terminal_safe(output: &mut String, value: &str, preserve_newlines: bool) {
    for character in value.chars() {
        match character {
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' if preserve_newlines => output.push('\n'),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                let _ = write!(output, "\\u{{{:x}}}", u32::from(character));
            }
            character => output.push(character),
        }
    }
}

fn push_indentation(output: &mut String, indentation: usize) {
    output.extend(std::iter::repeat_n(' ', indentation));
}

#[cfg(test)]
mod tests {
    use super::structured;
    use crate::args::OutputFormat;

    #[test]
    fn structured_text_escapes_untrusted_control_characters() {
        let value = serde_json::json!({
            "reference": "before\u{1b}[2Jafter\nforged\rline\tcolumn"
        });
        let rendered = structured(&value, OutputFormat::Text).expect("render");

        assert_eq!(
            rendered,
            "reference: before\\u{1b}[2Jafter\\nforged\\rline\\tcolumn\n"
        );
        assert!(
            rendered
                .strip_suffix('\n')
                .expect("renderer newline")
                .chars()
                .all(|character| !character.is_control())
        );
    }
}
