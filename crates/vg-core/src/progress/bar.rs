use std::{sync::Mutex, time::Duration};

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use super::ProgressReporter;

#[derive(Debug, Default)]
pub struct TerminalReporter {
    status_bar: Mutex<Option<ProgressBar>>,
    index_bar: Mutex<Option<ProgressBar>>,
    embed_bar: Mutex<Option<ProgressBar>>,
}

impl ProgressReporter for TerminalReporter {

    fn on_model_loading(&self, model_id: &str, description: &str) {
        let spinner = create_spinner(&format!("正在加载模型 {model_id} ({description})..."));
        *self
            .status_bar
            .lock()
            .expect("status progress lock poisoned") = Some(spinner);
    }

    fn on_model_loaded(&self) {
        if let Some(bar) = self
            .status_bar
            .lock()
            .expect("status progress lock poisoned")
            .take()
        {
            bar.finish_and_clear();
        }
    }

    fn on_scan_complete(&self, _total_files: usize) {}

    fn on_index_start(&self, total: usize) {
        if total == 0 {
            return;
        }
        let bar = create_bar(total, "预处理文件");
        *self.index_bar.lock().expect("index progress lock poisoned") = Some(bar);
    }

    fn on_index_tick(&self) {
        if let Some(bar) = self
            .index_bar
            .lock()
            .expect("index progress lock poisoned")
            .as_ref()
        {
            bar.inc(1);
        }
    }

    fn on_index_done(&self) {
        if let Some(bar) = self
            .index_bar
            .lock()
            .expect("index progress lock poisoned")
            .take()
        {
            bar.finish_and_clear();
        }
    }

    fn on_embed_start(&self, total: usize) {
        if total == 0 {
            return;
        }
        let bar = create_bar(total, "索引构建");
        *self.embed_bar.lock().expect("embed progress lock poisoned") = Some(bar);
    }

    fn on_embed_tick(&self) {
        if let Some(bar) = self
            .embed_bar
            .lock()
            .expect("embed progress lock poisoned")
            .as_ref()
        {
            bar.inc(1);
        }
    }

    fn on_embed_done(&self) {
        if let Some(bar) = self
            .embed_bar
            .lock()
            .expect("embed progress lock poisoned")
            .take()
        {
            bar.finish_and_clear();
        }
    }
}

fn create_spinner(message: &str) -> ProgressBar {
    let bar = ProgressBar::new_spinner();
    bar.set_draw_target(ProgressDrawTarget::stderr());
    bar.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .expect("progress template should be valid"),
    );
    bar.set_message(message.to_string());
    bar.enable_steady_tick(Duration::from_millis(80));
    bar
}

fn create_bar(total: usize, message: &str) -> ProgressBar {
    let bar = ProgressBar::new(total as u64);
    bar.set_draw_target(ProgressDrawTarget::stderr());
    bar.set_style(
        ProgressStyle::with_template("{msg} [{bar:30.cyan/blue}] {pos}/{len}")
            .expect("progress template should be valid")
            .progress_chars("=> "),
    );
    bar.set_message(message.to_string());
    bar
}

#[cfg(test)]
mod tests {
    use indicatif::ProgressDrawTarget;

    use super::{TerminalReporter, create_bar, create_spinner};
    use crate::progress::ProgressReporter;

    #[test]
    fn on_model_loaded_clears_status_spinner() {
        let reporter = TerminalReporter::default();
        let spinner = create_spinner("loading");
        spinner.set_draw_target(ProgressDrawTarget::hidden());
        *reporter
            .status_bar
            .lock()
            .expect("status progress lock poisoned") = Some(spinner);

        reporter.on_model_loaded();

        assert!(
            reporter
                .status_bar
                .lock()
                .expect("status progress lock poisoned")
                .is_none()
        );
    }

    #[test]
    fn done_handlers_clear_progress_bars() {
        let reporter = TerminalReporter::default();
        let index_bar = create_bar(2, "index");
        index_bar.set_draw_target(ProgressDrawTarget::hidden());
        *reporter
            .index_bar
            .lock()
            .expect("index progress lock poisoned") = Some(index_bar);

        let embed_bar = create_bar(2, "embed");
        embed_bar.set_draw_target(ProgressDrawTarget::hidden());
        *reporter
            .embed_bar
            .lock()
            .expect("embed progress lock poisoned") = Some(embed_bar);

        reporter.on_index_done();
        reporter.on_embed_done();

        assert!(
            reporter
                .index_bar
                .lock()
                .expect("index progress lock poisoned")
                .is_none()
        );
        assert!(
            reporter
                .embed_bar
                .lock()
                .expect("embed progress lock poisoned")
                .is_none()
        );
    }
}
