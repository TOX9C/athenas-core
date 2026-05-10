use gpui::*;

pub struct TerminalPaneView {
    pub title: String,
}

impl Render for TerminalPaneView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_col()
            .size_full()
            .border_1()
            .border_color(rgb(0x444444))
            .relative()
            .child(
                div().h(px(32.0)).bg(rgb(0x1a1a1a)).p_2().child(self.title.clone()) // Tab header
            )
            .child(
                div().flex_1().bg(rgb(0x000000)).p_2().child("alacritty_terminal view loads here...") // Terminal body
            )
    }
}
