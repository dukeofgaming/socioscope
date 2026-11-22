mod config;
mod library;
mod okf;
mod srt;
mod studio;
mod theme;

use gpui::{App, Bounds, QuitMode, WindowBounds, WindowOptions, prelude::*, px, size};

fn main() {
    gpui_platform::application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            studio::bind_keys(cx);
            let bounds = Bounds::centered(None, size(px(1520.), px(940.)), cx);
            let mut options = WindowOptions::default().window_bounds(WindowBounds::Windowed(bounds));
            options.window_min_size = Some(size(px(160.), px(160.)));
            cx.open_window(
                options,
                |window, cx| cx.new(|cx| studio::Studio::new(window, cx)),
            )
            .unwrap();
            cx.activate(true);
        });
}
