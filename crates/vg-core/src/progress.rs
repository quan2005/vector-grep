#[cfg(feature = "progress-bar")]
mod bar;

#[cfg(feature = "progress-bar")]
pub use bar::TerminalReporter;

pub trait ProgressReporter: Sync + Send {
    fn on_model_loading(&self, model_id: &str, description: &str);
    fn on_model_loaded(&self);
    fn on_scan_complete(&self, total_files: usize);
    fn on_index_start(&self, total: usize);
    fn on_index_tick(&self);
    fn on_index_done(&self);
    fn on_embed_start(&self, total: usize);
    fn on_embed_tick(&self);
    fn on_embed_done(&self);
}

#[derive(Debug, Default)]
pub struct NoopReporter;

impl ProgressReporter for NoopReporter {
    fn on_model_loading(&self, _model_id: &str, _description: &str) {}

    fn on_model_loaded(&self) {}

    fn on_scan_complete(&self, _total_files: usize) {}

    fn on_index_start(&self, _total: usize) {}

    fn on_index_tick(&self) {}

    fn on_index_done(&self) {}

    fn on_embed_start(&self, _total: usize) {}

    fn on_embed_tick(&self) {}

    fn on_embed_done(&self) {}
}

pub fn default_reporter() -> Box<dyn ProgressReporter> {
    #[cfg(feature = "progress-bar")]
    {
        Box::new(TerminalReporter::default())
    }

    #[cfg(not(feature = "progress-bar"))]
    {
        Box::new(NoopReporter)
    }
}
