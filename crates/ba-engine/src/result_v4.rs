//! Opt-in marginal first-acquisition reports. Embedded analyses retain schema 3.
use crate::{
    AcquisitionTimingOptions, ComparisonResultV3, ConfidenceInterval, ExactAnalysisResultV3,
    MonteCarloAnalysisResultV3,
};
use ba_core::StudentId;
use serde::Serialize;

pub const RESULT_SCHEMA_VERSION_V4: u64 = 4;

#[derive(Debug, Clone, Serialize)]
pub struct AcquisitionTimingPointV4 {
    pub additional_recruitment_count: u64,
    pub absolute_campaign_recruitment_count: u64,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SampledAcquisitionTimingPointV4 {
    pub additional_recruitment_count: u64,
    pub absolute_campaign_recruitment_count: u64,
    pub probability: f64,
    pub sample_count: u64,
    pub confidence_interval_95: ConfidenceInterval,
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetAcquisitionTimingV4 {
    pub target_index: usize,
    pub target_id: StudentId,
    pub initially_owned: bool,
    pub acquired_by_terminal_probability: f64,
    pub not_acquired_by_terminal_probability: f64,
    pub pmf: Vec<AcquisitionTimingPointV4>,
    pub cdf: Vec<AcquisitionTimingPointV4>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SampledTargetAcquisitionTimingV4 {
    pub target_index: usize,
    pub target_id: StudentId,
    pub initially_owned: bool,
    pub acquired_by_terminal_probability: f64,
    pub not_acquired_by_terminal_probability: f64,
    pub acquired_sample_count: u64,
    pub not_acquired_sample_count: u64,
    pub pmf: Vec<SampledAcquisitionTimingPointV4>,
    pub cdf: Vec<SampledAcquisitionTimingPointV4>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExactAcquisitionTimingV4 {
    pub options: AcquisitionTimingOptions,
    pub targets: Vec<TargetAcquisitionTimingV4>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonteCarloAcquisitionTimingV4 {
    pub options: AcquisitionTimingOptions,
    pub targets: Vec<SampledTargetAcquisitionTimingV4>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExactAcquisitionTimingResultV4 {
    pub result_schema_version: u64,
    pub engine_kind: &'static str,
    pub analysis: ExactAnalysisResultV3,
    pub acquisition_timing: ExactAcquisitionTimingV4,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonteCarloAcquisitionTimingResultV4 {
    pub result_schema_version: u64,
    pub engine_kind: &'static str,
    pub analysis: MonteCarloAnalysisResultV3,
    pub acquisition_timing: MonteCarloAcquisitionTimingV4,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcquisitionTimingComparisonPointV4 {
    pub additional_recruitment_count: u64,
    pub absolute_campaign_recruitment_count: u64,
    pub pmf_simulation_minus_exact: f64,
    pub cdf_simulation_minus_exact: f64,
    pub exact_pmf_within_monte_carlo_interval: bool,
    pub exact_cdf_within_monte_carlo_interval: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetAcquisitionTimingComparisonV4 {
    pub target_index: usize,
    pub target_id: StudentId,
    pub not_acquired_simulation_minus_exact: f64,
    pub exact_not_acquired_within_monte_carlo_interval: bool,
    pub points: Vec<AcquisitionTimingComparisonPointV4>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcquisitionTimingComparisonV4 {
    pub exact: ExactAcquisitionTimingV4,
    pub monte_carlo: MonteCarloAcquisitionTimingV4,
    pub comparisons: Vec<TargetAcquisitionTimingComparisonV4>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComparisonAcquisitionTimingResultV4 {
    pub result_schema_version: u64,
    pub engine_kind: &'static str,
    pub analysis: ComparisonResultV3,
    pub acquisition_timing: AcquisitionTimingComparisonV4,
}
