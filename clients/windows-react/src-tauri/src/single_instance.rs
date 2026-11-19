use tauri::{AppHandle, Manager, Runtime, WebviewWindow, Window, WindowEvent};

const MINIMUM_VISIBLE_WIDTH: i64 = 160;
const MINIMUM_VISIBLE_HEIGHT: i64 = 120;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PhysicalRectangle {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

impl PhysicalRectangle {
    fn right(self) -> i64 {
        self.x.saturating_add(self.width)
    }

    fn bottom(self) -> i64 {
        self.y.saturating_add(self.height)
    }
}

pub(crate) fn present_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.unminimize();
    restore_to_visible_monitor_if_needed(&window);
    let _ = window.show();
    let _ = window.set_focus();
}

pub(crate) fn handle_main_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}

fn restore_to_visible_monitor_if_needed<R: Runtime>(window: &WebviewWindow<R>) {
    let Ok(position) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let Ok(monitors) = window.available_monitors() else {
        return;
    };
    let window_rectangle = PhysicalRectangle {
        x: i64::from(position.x),
        y: i64::from(position.y),
        width: i64::from(size.width),
        height: i64::from(size.height),
    };
    let visible = monitors.iter().any(|monitor| {
        let position = monitor.position();
        let size = monitor.size();
        meaningfully_intersects(
            window_rectangle,
            PhysicalRectangle {
                x: i64::from(position.x),
                y: i64::from(position.y),
                width: i64::from(size.width),
                height: i64::from(size.height),
            },
        )
    });
    if !visible {
        let _ = window.center();
    }
}

fn meaningfully_intersects(window: PhysicalRectangle, monitor: PhysicalRectangle) -> bool {
    if window.width <= 0 || window.height <= 0 || monitor.width <= 0 || monitor.height <= 0 {
        return false;
    }
    let width = window.right().min(monitor.right()) - window.x.max(monitor.x);
    let height = window.bottom().min(monitor.bottom()) - window.y.max(monitor.y);
    width >= MINIMUM_VISIBLE_WIDTH && height >= MINIMUM_VISIBLE_HEIGHT
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: PhysicalRectangle = PhysicalRectangle {
        x: 0,
        y: 0,
        width: 1_920,
        height: 1_080,
    };

    #[test]
    fn requires_a_meaningful_visible_region() {
        assert!(meaningfully_intersects(
            PhysicalRectangle {
                x: 1_760,
                y: 960,
                width: 1_180,
                height: 760,
            },
            PRIMARY,
        ));
        assert!(!meaningfully_intersects(
            PhysicalRectangle {
                x: 1_900,
                y: 1_060,
                width: 1_180,
                height: 760,
            },
            PRIMARY,
        ));
    }

    #[test]
    fn supports_monitors_left_of_the_primary_display() {
        let left = PhysicalRectangle {
            x: -1_920,
            y: 0,
            width: 1_920,
            height: 1_080,
        };
        assert!(meaningfully_intersects(
            PhysicalRectangle {
                x: -1_850,
                y: 80,
                width: 1_180,
                height: 760,
            },
            left,
        ));
    }

    #[test]
    fn rejects_invalid_geometry() {
        assert!(!meaningfully_intersects(
            PhysicalRectangle {
                x: 0,
                y: 0,
                width: 0,
                height: 760,
            },
            PRIMARY,
        ));
    }
}
