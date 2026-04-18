use anyhow::Result;
use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

pub struct FileWatcher {
    _debouncer: Debouncer<notify_debouncer_full::notify::RecommendedWatcher, RecommendedCache>,
}

impl FileWatcher {
    pub fn new(path: PathBuf, tx: Sender<()>) -> Result<Self> {
        let path_clone = path.clone();
        let mut debouncer = new_debouncer(
            Duration::from_millis(300),
            None,
            move |result: DebounceEventResult| {
                if let Ok(events) = result {
                    for event in events {
                        let matches = event.paths.iter().any(|p| {
                            // ファイルパスが一致する場合のみ通知
                            p == &path_clone
                                || p.canonicalize().ok() == path_clone.canonicalize().ok()
                        });
                        if matches {
                            let _ = tx.send(());
                            break;
                        }
                    }
                }
            },
        )?;

        // 親ディレクトリを監視（エディタのrename+create対応）
        let watch_dir = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        debouncer.watch(&watch_dir, RecursiveMode::NonRecursive)?;

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}
