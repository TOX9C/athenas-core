use gpui::*;

pub struct SidebarView;

impl Render for SidebarView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(250.0))
            .h_full()
            .bg(rgb(0x1a1a1a))
            .border_r_1()
            .border_color(rgb(0x333333))
            .flex_col()
            .child(
                div().p_4().text_color(rgb(0xcccccc)).child("WORKSPACES")
            )
            .child(
                div().px_4().py_2().text_color(rgb(0xffffff)).child("• Dev Space")
            )
    }
}
