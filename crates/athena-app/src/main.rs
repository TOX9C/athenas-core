use gpui::*;

struct AthenaApp;

impl Render for AthenaApp {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .child("Athena Grid Shell Loading...")
    }
}

fn main() {
    let app = gpui::App::new();
    app.run(|cx: &mut AppContext| {
        let options = WindowOptions::default();
        cx.open_window(options, |cx| {
            cx.new_view(|_cx| AthenaApp)
        });
    });
}