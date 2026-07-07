// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::Ordering;

use anyhow::{anyhow, Result};
use portable_pty::PtySize;

use super::LocalPtySession;

impl LocalPtySession {
    pub(crate) fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let master = self
            .master
            .lock()
            .map_err(|_| anyhow!("terminal pty lock failed"))?;
        master
            .resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| anyhow!("resize terminal failed: {err}"))
    }

    pub(crate) fn snapshot(&self, lines: usize) -> String {
        let history = match self.output_history.lock() {
            Ok(history) => history.clone(),
            Err(_) => return String::new(),
        };
        let normalized = lines.clamp(1, 10_000);
        let mut items = history.lines().rev().take(normalized).collect::<Vec<_>>();
        items.reverse();
        items.join("\n")
    }

    pub(crate) fn busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    pub(in crate::terminal::session) fn append_output(&self, data: &str) {
        const MAX_HISTORY_BYTES: usize = 1024 * 1024;
        let Ok(mut history) = self.output_history.lock() else {
            return;
        };
        history.push_str(data);
        if history.len() > MAX_HISTORY_BYTES {
            let trim_to = history.len().saturating_sub(MAX_HISTORY_BYTES);
            let mut boundary = trim_to;
            while boundary < history.len() && !history.is_char_boundary(boundary) {
                boundary += 1;
            }
            history.drain(..boundary);
        }
    }

    pub(in crate::terminal::session) fn close(&self) {
        self.exited.store(true, Ordering::SeqCst);
        if let Ok(mut killer) = self.child_killer.lock() {
            let _ = killer.kill();
        }
    }
}
