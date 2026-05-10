use gpui::*;
use crate::terminal_pane::TerminalPaneView;

pub enum Split {
    Horizontal(Box<Split>, Box<Split>),
    Vertical(Box<Split>, Box<Split>),
    Leaf(usize),
}

pub struct PaneGridView {
    pub root_split: Option<Split>,
    pub terminal: Entity<TerminalPaneView>,
}

impl PaneGridView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            root_split: None,
            terminal: cx.new(|_| TerminalPaneView {
                title: "Terminal 1".into(),
            }),
        }
    }
}

impl Render for PaneGridView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .child(self.terminal.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn test_grid_tree() {
        let split = Split::Horizontal(
            Box::new(Split::Leaf(1)),
            Box::new(Split::Leaf(2))
        );
        assert!(matches!(split, Split::Horizontal(_, _)));
    }
}
