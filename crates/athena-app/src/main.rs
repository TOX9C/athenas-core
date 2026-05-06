use gpui::*;

struct HelloWorld;

impl Render for HelloWorld {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .flex()
            .bg(rgb(0x0a0a0a))
            .size_full()
            .justify_center()
            .items_center()
            .text_xl()
            .text_color(rgb(0xffffff))
            .child("Athena's Core (Native Rust)")
    }
}

fn main() {
    App::new().run(|cx: &mut AppContext| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(800.0), px(600.0)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Athena's Core".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        };
        cx.open_window(options, |cx| {
            cx.new_view(|_cx| HelloWorld)
        });
    });
}
