//! Host-side demo chrome shared by the gallery and streaming dashboard. It is
//! not part of a program: it owns background, page padding and the optional
//! developer inspector.

use crate::{
    inspector::{self, InspectorInput, InspectorView},
    theme::UiTheme,
    view::UiView,
};
use gpui_kit::{
    AnyElement, AnyView, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Window, div,
};

pub struct DemoStage {
    pub theme: UiTheme,
    pub child: Option<Entity<UiView>>,
    pub inspector: Option<Entity<InspectorView>>,
    pub caption: Option<String>,
    pub show_inspector: bool,
}

impl DemoStage {
    pub fn new(theme: UiTheme) -> Self {
        Self {
            theme,
            child: None,
            inspector: None,
            caption: None,
            show_inspector: false,
        }
    }

    pub fn set_program(&mut self, child: Entity<UiView>, caption: Option<String>) {
        self.child = Some(child);
        self.caption = caption;
        self.inspector = None;
    }

    /// Rebuilds the inspector from the current child's retained state.
    pub fn refresh_inspector(&mut self, cx: &mut Context<Self>) {
        self.inspector = None;
        if !self.show_inspector {
            return;
        }
        let Some(child) = &self.child else {
            return;
        };
        let tree = child.read(cx).tree_handle();
        let metrics = child.read(cx).metrics_handle();
        let diagnostics = child.read(cx).diagnostics().to_vec();
        let theme = self.theme.clone();
        self.inspector = Some(cx.new(|_| InspectorView {
            tree,
            metrics,
            input: InspectorInput { diagnostics },
            theme,
        }));
    }
}

impl Render for DemoStage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let program: AnyElement = match &self.child {
            Some(child) => AnyView::from(child.clone()).into_any_element(),
            None => {
                let style = self.theme.text(crate::theme::TextRole::Body);
                div()
                    .text_size(style.size)
                    .line_height(style.line_height)
                    .text_color(self.theme.muted_foreground())
                    .child("No program loaded")
                    .into_any_element()
            }
        };
        let mut column = div().flex().flex_col().gap(self.theme.space(3)).w_full();
        if let Some(caption) = &self.caption {
            let style = self.theme.text(crate::theme::TextRole::Caption);
            column = column.child(
                div()
                    .text_size(style.size)
                    .line_height(style.line_height)
                    .text_color(self.theme.muted_foreground())
                    .child(caption.clone()),
            );
        }
        let program = column.child(program).into_any_element();
        let inspector = self
            .inspector
            .as_ref()
            .map(|inspector| AnyView::from(inspector.clone()).into_any_element());
        inspector::shell(&self.theme, program, inspector)
    }
}

impl DemoStage {
    /// The default stage size for one example (DESIGN §6 rule 1 max width).
    pub fn window_size(theme: &UiTheme) -> gpui_kit::Size<gpui_kit::Pixels> {
        theme.demo_window_size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_starts_without_a_program_or_inspector() {
        let stage = DemoStage::new(UiTheme::for_mode(crate::theme::ThemeMode::Light));
        assert!(stage.child.is_none());
        assert!(stage.inspector.is_none());
    }
}
