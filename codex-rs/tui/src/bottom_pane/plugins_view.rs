use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::Widget;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::key_hint;
use crate::render::Insets;
use crate::render::RectExt as _;
use crate::render::renderable::ColumnRenderable;
use crate::render::renderable::Renderable;
use crate::style::user_message_style;
use codex_core::plugins::PluginScope;

use super::CancellationEvent;
use super::bottom_pane_view::BottomPaneView;
use super::popup_consts::MAX_POPUP_ROWS;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::render_rows_single_line;

#[derive(Clone)]
pub(crate) struct PluginToggleItem {
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub scope: PluginScope,
    pub compliance_hint: Option<String>,
}

pub(crate) struct PluginsToggleView {
    items: Vec<PluginToggleItem>,
    state: ScrollState,
    complete: bool,
    app_event_tx: AppEventSender,
    header: Box<dyn Renderable>,
    footer_hint: Line<'static>,
}

impl PluginsToggleView {
    pub(crate) fn new(items: Vec<PluginToggleItem>, app_event_tx: AppEventSender) -> Self {
        let mut header = ColumnRenderable::new();
        header.push(Line::from("Enable/Disable Plugins".bold()));
        header.push(Line::from(
            "Toggle plugins on or off. Policy settings are managed separately.".dim(),
        ));

        let mut view = Self {
            items,
            state: ScrollState::new(),
            complete: false,
            app_event_tx,
            header: Box::new(header),
            footer_hint: plugins_toggle_hint_line(),
        };
        if !view.items.is_empty() {
            view.state.selected_idx = Some(0);
        }
        view
    }

    fn visible_len(&self) -> usize {
        self.items.len()
    }

    fn max_visible_rows(len: usize) -> usize {
        MAX_POPUP_ROWS.min(len.max(1))
    }

    fn build_rows(&self) -> Vec<GenericDisplayRow> {
        self.items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let is_selected = self.state.selected_idx == Some(idx);
                let prefix = if is_selected { '›' } else { ' ' };
                let marker = if item.enabled { 'x' } else { ' ' };
                let scope = scope_label(item.scope.clone());
                let name = format!(
                    "{prefix} [{marker}] {plugin_name} ({scope})",
                    plugin_name = item.name
                );
                let description = if let Some(hint) = item.compliance_hint.as_deref() {
                    if hint.is_empty() {
                        item.description.clone()
                    } else {
                        format!("{description} · {hint}", description = item.description)
                    }
                } else {
                    item.description.clone()
                };
                GenericDisplayRow {
                    name,
                    description: Some(description),
                    ..Default::default()
                }
            })
            .collect()
    }

    fn move_up(&mut self) {
        let len = self.visible_len();
        self.state.move_up_wrap(len);
        let visible = Self::max_visible_rows(len);
        self.state.ensure_visible(len, visible);
    }

    fn move_down(&mut self) {
        let len = self.visible_len();
        self.state.move_down_wrap(len);
        let visible = Self::max_visible_rows(len);
        self.state.ensure_visible(len, visible);
    }

    fn toggle_selected(&mut self) {
        let Some(idx) = self.state.selected_idx else {
            return;
        };
        let Some(item) = self.items.get_mut(idx) else {
            return;
        };

        // 关键逻辑：更新本地状态后通知应用层落盘写入 registry。
        item.enabled = !item.enabled;
        self.app_event_tx.send(AppEvent::SetPluginEnabled {
            name: item.name.clone(),
            scope: item.scope.clone(),
            enabled: item.enabled,
        });
    }

    fn close(&mut self) {
        if self.complete {
            return;
        }
        self.complete = true;
        self.app_event_tx.send(AppEvent::ManagePluginsClosed);
        self.app_event_tx
            .send(AppEvent::CodexOp(codex_core::protocol::Op::ListSkills {
                cwds: Vec::new(),
                force_reload: true,
            }));
        self.app_event_tx.send(AppEvent::CodexOp(
            codex_core::protocol::Op::ListCustomPrompts,
        ));
    }

    fn rows_width(total_width: u16) -> u16 {
        total_width.saturating_sub(2)
    }

    fn rows_height(&self, rows: &[GenericDisplayRow]) -> u16 {
        rows.len().clamp(1, MAX_POPUP_ROWS).try_into().unwrap_or(1)
    }
}

impl BottomPaneView for PluginsToggleView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('\u{0010}'),
                modifiers: KeyModifiers::NONE,
                ..
            } /* ^P */ => self.move_up(),
            KeyEvent {
                code: KeyCode::Down,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('\u{000e}'),
                modifiers: KeyModifiers::NONE,
                ..
            } /* ^N */ => self.move_down(),
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => self.toggle_selected(),
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.on_ctrl_c();
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.close();
        CancellationEvent::Handled
    }
}

impl Renderable for PluginsToggleView {
    fn desired_height(&self, width: u16) -> u16 {
        let rows = self.build_rows();
        let rows_height = self.rows_height(&rows);

        let mut height = self.header.desired_height(width.saturating_sub(4));
        height = height.saturating_add(rows_height + 3);
        height = height.saturating_add(2);
        height.saturating_add(1)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let [content_area, footer_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

        Block::default()
            .style(user_message_style())
            .render(content_area, buf);

        let header_height = self
            .header
            .desired_height(content_area.width.saturating_sub(4));
        let rows = self.build_rows();
        let rows_width = Self::rows_width(content_area.width);
        let rows_height = self.rows_height(&rows);
        let [header_area, _, list_area] = Layout::vertical([
            Constraint::Max(header_height),
            Constraint::Max(1),
            Constraint::Length(rows_height),
        ])
        .areas(content_area.inset(Insets::vh(1, 2)));

        self.header.render(header_area, buf);

        if list_area.height > 0 {
            let render_area = Rect {
                x: list_area.x.saturating_sub(2),
                y: list_area.y,
                width: rows_width.max(1),
                height: list_area.height,
            };
            render_rows_single_line(
                render_area,
                buf,
                &rows,
                &self.state,
                render_area.height as usize,
                "no plugins",
            );
        }

        let hint_area = Rect {
            x: footer_area.x + 2,
            y: footer_area.y,
            width: footer_area.width.saturating_sub(2),
            height: footer_area.height,
        };
        self.footer_hint.clone().dim().render(hint_area, buf);
    }
}

fn plugins_toggle_hint_line() -> Line<'static> {
    Line::from(vec![
        "Press ".into(),
        key_hint::plain(KeyCode::Char(' ')).into(),
        " or ".into(),
        key_hint::plain(KeyCode::Enter).into(),
        " to toggle; ".into(),
        key_hint::plain(KeyCode::Esc).into(),
        " to close".into(),
    ])
}

fn scope_label(scope: PluginScope) -> &'static str {
    match scope {
        PluginScope::User => "user",
        PluginScope::Project => "project",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use ratatui::layout::Rect;
    use tokio::sync::mpsc::unbounded_channel;

    fn render_lines(view: &PluginsToggleView, width: u16) -> String {
        let height = view.desired_height(width);
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        let lines: Vec<String> = (0..area.height)
            .map(|row| {
                let mut line = String::new();
                for col in 0..area.width {
                    let symbol = buf[(area.x + col, area.y + row)].symbol();
                    if symbol.is_empty() {
                        line.push(' ');
                    } else {
                        line.push_str(symbol);
                    }
                }
                line
            })
            .collect();

        lines.join("\n")
    }

    #[test]
    fn renders_basic_popup() {
        let (tx_raw, _rx) = unbounded_channel();
        let tx = AppEventSender::new(tx_raw);
        let items = vec![PluginToggleItem {
            name: "everything-claude-code".to_string(),
            description: "commands, skills".to_string(),
            enabled: true,
            scope: PluginScope::User,
            compliance_hint: Some("hooks detected".to_string()),
        }];

        let view = PluginsToggleView::new(items, tx);
        assert_snapshot!(render_lines(&view, 60));
    }
}
