// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::c_void;

use super::{CFRelease, CGEventPost, CGEventSetLocation, CGPoint};

pub(super) struct CoreGraphicsUpGuard {
    event: *mut c_void,
}

impl CoreGraphicsUpGuard {
    pub(super) fn new(event: *mut c_void) -> Self {
        Self { event }
    }

    pub(super) fn set_location(&mut self, point: CGPoint) {
        // SAFETY: event is retained and owned by this guard until release or drop.
        unsafe { CGEventSetLocation(self.event, point) };
    }

    pub(super) fn release(mut self) {
        self.post_and_release();
    }

    fn post_and_release(&mut self) {
        if self.event.is_null() {
            return;
        }
        // SAFETY: event is a retained CoreGraphics event owned by this guard. It is posted and
        // released exactly once, then cleared so Drop is idempotent.
        unsafe {
            CGEventPost(0, self.event);
            CFRelease(self.event);
        }
        self.event = std::ptr::null_mut();
    }
}

impl Drop for CoreGraphicsUpGuard {
    fn drop(&mut self) {
        self.post_and_release();
    }
}
