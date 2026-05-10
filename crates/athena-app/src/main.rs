#![recursion_limit = "512"]
use gpui::*;
use gpui_platform::application;

mod sidebar;
use sidebar::SidebarView;

mod terminal_pane;
mod pane_grid;
use pane_grid::{PaneGridView, Split};

struct AthenaWorkspace {
    sidebar: Entity<SidebarView>,
    pane_grid: Entity<PaneGridView>,
}

impl AthenaWorkspace {
    pub fn build(cx: &mut Context<Self>) -> Self {
        AthenaWorkspace {
            sidebar: cx.new(|_cx| SidebarView),
            pane_grid: cx.new(|cx| PaneGridView::new(cx)),
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
                    .p_4().child(self.pane_grid.clone())
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
