use crate::{args::OutputFormat, errors::AppError, render};
use ba_engine::{
    ComparisonAcquisitionTimingResultV4, ExactAcquisitionTimingResultV4, ExactAcquisitionTimingV4,
    MonteCarloAcquisitionTimingResultV4, MonteCarloAcquisitionTimingV4,
};
use serde::Serialize;
use std::io::{self, Write};

const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}
impl BoundedWriter {
    fn new(maximum: usize) -> Self {
        Self {
            bytes: Vec::new(),
            maximum,
            exceeded: false,
        }
    }
    fn finish(self, status: Result<(), impl std::fmt::Display>) -> Result<String, AppError> {
        if self.exceeded {
            return Err(AppError::OutputSizeLimitExceeded {
                maximum: self.maximum,
            });
        }
        status.map_err(|e| AppError::Internal(format!("timing rendering failed: {e}")))?;
        String::from_utf8(self.bytes)
            .map_err(|e| AppError::Internal(format!("timing output is not UTF-8: {e}")))
    }
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.maximum - self.bytes.len() {
            self.exceeded = true;
            return Err(io::Error::other("timing output size limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn json(value: &impl Serialize, maximum: usize) -> Result<String, AppError> {
    let mut writer = BoundedWriter::new(maximum);
    let status = serde_json::to_writer_pretty(&mut writer, value)
        .map_err(io::Error::other)
        .and_then(|()| writer.write_all(b"\n"));
    writer.finish(status)
}
fn text(
    summary: &str,
    append: impl FnOnce(&mut BoundedWriter) -> io::Result<()>,
) -> Result<String, AppError> {
    let mut writer = BoundedWriter::new(MAX_OUTPUT_BYTES);
    let status = writer
        .write_all(summary.as_bytes())
        .and_then(|()| append(&mut writer));
    writer.finish(status)
}

pub(crate) fn exact(
    value: &ExactAcquisitionTimingResultV4,
    format: OutputFormat,
) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => json(value, MAX_OUTPUT_BYTES),
        OutputFormat::Text => text(&render::exact_v3(&value.analysis, format)?, |writer| {
            exact_rows(writer, &value.acquisition_timing)
        }),
    }
}
pub(crate) fn monte_carlo(
    value: &MonteCarloAcquisitionTimingResultV4,
    format: OutputFormat,
) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => json(value, MAX_OUTPUT_BYTES),
        OutputFormat::Text => text(
            &render::monte_carlo_v3(&value.analysis, format)?,
            |writer| sampled_rows(writer, &value.acquisition_timing),
        ),
    }
}
pub(crate) fn comparison(
    value: &ComparisonAcquisitionTimingResultV4,
    format: OutputFormat,
) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => json(value, MAX_OUTPUT_BYTES),
        OutputFormat::Text => text(&render::comparison_v3(&value.analysis, format)?, |writer| {
            exact_rows(writer, &value.acquisition_timing.exact)?;
            sampled_rows(writer, &value.acquisition_timing.monte_carlo)?;
            writeln!(writer, "Timing comparison (pointwise 95% intervals):")?;
            for target in &value.acquisition_timing.comparisons {
                writeln!(
                    writer,
                    "Target {}: {}\nUnacquired difference: {:.15}; exact inside interval: {}",
                    target.target_index,
                    render::terminal_safe(target.target_id.as_str()),
                    target.not_acquired_simulation_minus_exact,
                    target.exact_not_acquired_within_monte_carlo_interval
                )?;
                writeln!(
                    writer,
                    "additional absolute PMF_difference CDF_difference PMF_inside CDF_inside"
                )?;
                for point in &target.points {
                    writeln!(
                        writer,
                        "{} {} {:.15} {:.15} {} {}",
                        point.additional_recruitment_count,
                        point.absolute_campaign_recruitment_count,
                        point.pmf_simulation_minus_exact,
                        point.cdf_simulation_minus_exact,
                        point.exact_pmf_within_monte_carlo_interval,
                        point.exact_cdf_within_monte_carlo_interval
                    )?;
                }
            }
            Ok(())
        }),
    }
}
fn exact_rows(writer: &mut BoundedWriter, timing: &ExactAcquisitionTimingV4) -> io::Result<()> {
    writeln!(writer, "Acquisition timing (exact, unconditional):")?;
    for target in &timing.targets {
        writeln!(
            writer,
            "Target {}: {}\nInitially owned: {}\nAcquired by terminal: {:.15}\nNot acquired by terminal: {:.15}",
            target.target_index,
            render::terminal_safe(target.target_id.as_str()),
            target.initially_owned,
            target.acquired_by_terminal_probability,
            target.not_acquired_by_terminal_probability
        )?;
        writeln!(writer, "additional absolute PMF CDF")?;
        for (p, c) in target.pmf.iter().zip(&target.cdf) {
            writeln!(
                writer,
                "{} {} {:.15} {:.15}",
                p.additional_recruitment_count,
                p.absolute_campaign_recruitment_count,
                p.probability,
                c.probability
            )?;
        }
    }
    Ok(())
}
fn sampled_rows(
    writer: &mut BoundedWriter,
    timing: &MonteCarloAcquisitionTimingV4,
) -> io::Result<()> {
    writeln!(
        writer,
        "Acquisition timing (sampled, unconditional; pointwise 95% intervals):"
    )?;
    for target in &timing.targets {
        writeln!(
            writer,
            "Target {}: {}\nInitially owned: {}\nAcquired by terminal: {:.15} ({} samples)\nNot acquired by terminal: {:.15} ({} samples)",
            target.target_index,
            render::terminal_safe(target.target_id.as_str()),
            target.initially_owned,
            target.acquired_by_terminal_probability,
            target.acquired_sample_count,
            target.not_acquired_by_terminal_probability,
            target.not_acquired_sample_count
        )?;
        writeln!(
            writer,
            "additional absolute PMF CDF PMF_samples CDF_samples PMF_interval CDF_interval"
        )?;
        for (p, c) in target.pmf.iter().zip(&target.cdf) {
            writeln!(
                writer,
                "{} {} {:.15} {:.15} {} {} [{:.15}, {:.15}] [{:.15}, {:.15}]",
                p.additional_recruitment_count,
                p.absolute_campaign_recruitment_count,
                p.probability,
                c.probability,
                p.sample_count,
                c.sample_count,
                p.confidence_interval_95.lower,
                p.confidence_interval_95.upper,
                c.confidence_interval_95.lower,
                c.confidence_interval_95.upper
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_and_text_caps_count_utf8_bytes_and_newline_before_delivery() {
        let rendered = json(&"青", 100).unwrap();
        assert_eq!(json(&"青", rendered.len()).unwrap(), rendered);
        let error = json(&"青", rendered.len() - 1).unwrap_err();
        let classified = crate::errors::classify_error(error);
        assert_eq!(classified.exit, 5);
        assert_eq!(classified.body.code, "output_size_limit_exceeded");
        let mut writer = BoundedWriter::new(3);
        writer.write_all("青".as_bytes()).unwrap();
        let status = writeln!(writer);
        assert!(matches!(
            writer.finish(status),
            Err(AppError::OutputSizeLimitExceeded { .. })
        ));
    }
}

#[cfg(test)]
mod report_limit_tests {
    use super::*;
    #[test]
    fn a_large_schema_four_comparison_is_rejected_by_the_production_json_cap() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ba_core::AnyValidatedScenarioBundle::V3(bundle) = ba_core::load_any_bundle(
            root.join("data"),
            root.join("scenarios/golden/v3_atomic_cross_target.json"),
        )
        .unwrap() else {
            panic!("v3")
        };
        let mut report = ba_engine::compare_v3_with_acquisition_timing(
            &bundle,
            std::num::NonZeroU64::new(11).unwrap(),
            42,
            Default::default(),
            Default::default(),
            Default::default(),
        )
        .unwrap();
        // Worst-case DTO volume: each dataset can independently reach its cap.
        // Expand the first target only, keeping total support within 65,536.
        let n = 65_536 - report.acquisition_timing.exact.targets[1].pmf.len();
        let t = &mut report.acquisition_timing.exact.targets[0];
        t.pmf.resize(n, t.pmf[0].clone());
        t.cdf.resize(n, t.cdf[0].clone());
        let n = 65_536 - report.acquisition_timing.monte_carlo.targets[1].pmf.len();
        let t = &mut report.acquisition_timing.monte_carlo.targets[0];
        t.pmf.resize(n, t.pmf[0].clone());
        t.cdf.resize(n, t.cdf[0].clone());
        let n = 65_536 - report.acquisition_timing.comparisons[1].points.len();
        let t = &mut report.acquisition_timing.comparisons[0];
        t.points.resize(n, t.points[0].clone());
        assert!(matches!(
            comparison(&report, OutputFormat::Json),
            Err(AppError::OutputSizeLimitExceeded {
                maximum: MAX_OUTPUT_BYTES
            })
        ));
    }
}
