use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// A new step started; `total` is the number of items in it (0 if unknown).
    Stage { name: String, total: u64 },
    /// `n` more items of the current step finished.
    Advance(u64),
}

/// Cheap-to-clone progress reporter. The CLI draws a progress bar from it and
/// the UI will forward it to the frontend.
#[derive(Clone, Default)]
pub struct Progress(Option<Arc<dyn Fn(ProgressEvent) + Send + Sync>>);

impl Progress {
    pub fn new(f: impl Fn(ProgressEvent) + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(f)))
    }

    pub fn none() -> Self {
        Self(None)
    }

    pub fn stage(&self, name: &str, total: u64) {
        self.emit(ProgressEvent::Stage {
            name: name.to_string(),
            total,
        });
    }

    pub fn advance(&self, n: u64) {
        self.emit(ProgressEvent::Advance(n));
    }

    fn emit(&self, event: ProgressEvent) {
        if let Some(f) = &self.0 {
            f(event);
        }
    }
}
