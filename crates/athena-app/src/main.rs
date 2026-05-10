use gpui::*;
use gpui_platform::application;

mod sidebar;
use sidebar::SidebarView;

struct AthenaWorkspace {
    sidebar: Entity<SidebarView>,
}

impl AthenaWorkspace {
    pub fn build(cx: &mut Context<Self>) -> Self {
        AthenaWorkspace {
            sidebar: cx.new(|_cx| SidebarView),
        }
    }
}

impl Render for AthenaWorkspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .child(self.sidebar.clone())
            .child(
                div().flex_1().bg(rgb(0x222222))
                    .p_4().child("Pane Grid Container (Terminal)")
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let options = WindowOptions::default();
        cx.open_window(
            options,
            |_, cx| {
                cx.new(|cx| AthenaWorkspace::build(cx))
            }
        ).unwrap();
    });
}
