mod json;
mod rg_style;
pub(crate) mod vector_teaser;

use anyhow::Result;

use crate::{index::SyncReport, search::SearchResponse, store::ScopeStats};

pub use json::render_json;
pub use rg_style::render_rg_style;

pub fn render_response(
    response: &SearchResponse,
    json: bool,
    show_score: bool,
    context_lines: usize,
) -> Result<()> {
    if json {
        render_json(response)
    } else {
        render_rg_style(response, show_score, context_lines)
    }
}

pub fn print_index_report(report: &SyncReport) {
    println!(
        "indexed={} removed={} chunks_total={} index_time_ms={}",
        report.files_indexed, report.files_removed, report.chunks_total, report.index_time_ms
    );
}

pub fn print_index_stats(stats: &ScopeStats) {
    println!(
        "files_total={} chunks_total={}",
        stats.files_total, stats.chunks_total
    );
}
