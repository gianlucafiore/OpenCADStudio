//! CAD-style application/File menu: command actions and recent drawings.
//!
//! All operations route to the existing command dispatcher or OpenRecent.
//! This panel must never read/parse DWGs during view construction.

use std::path::PathBuf;
use iced::widget::{button, column, container, mouse_area, row, scrollable, text, Space};
use iced::{Background, Border, Color, Element, Fill, Length, Theme};
use crate::app::Message;
use crate::ui::icons;

const MENU_WIDTH: f32 = 770.0;
const ACTION_WIDTH: f32 = 285.0;

/// Real, already-supported file commands; no shadow file I/O here.
const ACTIONS: &[(&str, &str, &[u8])] = &[
    ("New drawing", "QNEW", icons::DOC_NEW),
    ("New from template", "NEW", icons::DOC_NEW),
    ("Open", "OPEN", icons::FOLDER_OPEN),
    ("Save", "QSAVE", icons::SAVE),
    ("Save As", "SAVEAS", icons::FILE_EXPORT),
    ("Save All", "SAVEALL", icons::SAVE),
    ("Close drawing", "CLOSE", icons::CLOSE),
    ("Import", "IMPORT", icons::FOLDER_OPEN),
    ("Export", "EXPORT", icons::FILE_EXPORT),
    ("Plot", "PLOT", icons::PRINT),
    ("Publish", "PUBLISH", icons::PRINT),
    ("eTransmit", "ETRANSMIT", icons::FILE_EXPORT),
    ("Drawing audit", "AUDIT", icons::DOC),
    ("Purge", "PURGE", icons::DOC),
    ("Options", "OPTIONS", icons::MENU),
    ("Exit", "QUIT", icons::CLOSE),
];

fn action<'a>(label: &'static str, command: &'static str, icon: &'static [u8]) -> Element<'a, Message> {
    let content = row![
        icons::themed(icon, 18.0),
        text(label).size(13),
    ]
    .spacing(11)
    .align_y(iced::Center);
    button(content)
        .width(Fill)
        .height(35)
        .padding([6, 12])
        .on_press(Message::AppFileMenuCommand(command.to_owned()))
        .style(|theme: &Theme, status| {
            let p = theme.palette();
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: hovered.then_some(Background::Color(p.primary.weak.color)),
                text_color: p.background.base.text,
                border: Border { radius: 4.0.into(), ..Border::default() },
                ..button::Style::default()
            }
        })
        .into()
}

fn recent<'a>(path: &'a PathBuf, index: usize) -> Element<'a, Message> {
    let name = path.file_name()
        .map(|part| part.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let folder = path.parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();
    button(
        row![
            container(text(format!("{}", index + 1)).size(12))
                .width(Length::Fixed(24.0))
                .align_x(iced::Center),
            column![
                text(name).size(13),
                text(folder).size(10).style(|theme: &Theme| {
                    iced::widget::text::Style {
                        color: Some(theme.palette().background.base.text.scale_alpha(0.55)),
                    }
                }),
            ]
            .spacing(2)
            .width(Fill),
        ]
        .spacing(8)
        .align_y(iced::Center)
    )
    .on_press(Message::AppFileMenuRecent(path.clone()))
    .width(Fill)
    .padding([9, 6])
    .style(|theme: &Theme, status| {
        let p = theme.palette();
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: hovered.then_some(Background::Color(p.primary.weak.color)),
            text_color: p.background.base.text,
            border: Border { radius: 4.0.into(), ..Border::default() },
            ..button::Style::default()
        }
    })
    .into()
}

fn heading<'a>(label: &'static str) -> Element<'a, Message> {
    container(text(label).size(12).style(|theme: &Theme| {
        iced::widget::text::Style {
            color: Some(theme.palette().background.base.text.scale_alpha(0.7)),
        }
    }))
    .padding([12, 12])
    .into()
}

/// Full-window dismiss surface with an upper-left two-column file panel.
/// The opaque panel captures pointer presses so a click cannot reach the DWG
/// while the menu is visible.
pub fn view<'a>(recents: &'a [PathBuf], window_size: (f32, f32)) -> Element<'a, Message> {
    let w = (window_size.0 - 12.0).min(MENU_WIDTH).max(260.0);
    let h = (window_size.1 - 12.0).min(720.0).max(240.0);
    let wide = w >= 580.0;
    let actions = {
        let mut entries: Vec<Element<'a, Message>> = Vec::new();
        entries.push(heading("DRAWINGS"));
        for (label, command, icon) in &ACTIONS[0..7] {
            entries.push(action(label, command, icon));
        }
        entries.push(heading("TRANSFER & OUTPUT"));
        for (label, command, icon) in &ACTIONS[7..12] {
            entries.push(action(label, command, icon));
        }
        entries.push(heading("TOOLS"));
        for (label, command, icon) in &ACTIONS[12..] {
            entries.push(action(label, command, icon));
        }
        container(scrollable(column(entries).spacing(1)).height(Fill))
            .width(if wide { Length::Fixed(ACTION_WIDTH) } else { Fill })
            .height(Fill)
    };
    let recents_list = if recents.is_empty() {
        column![text("No recent drawings yet.").size(12)].padding(12)
    } else {
        let mut entries = column![].spacing(3);
        for (index, path) in recents.iter().enumerate() {
            entries = entries.push(recent(path, index));
        }
        entries
    };
    let recent_panel = container(column![
        row![
            text("Recent Documents").size(17),
            Space::new().width(Fill),
            button(icons::themed(icons::CLOSE, 14.0))
                .on_press(Message::AppFileMenuClose)
                .padding(5)
                .style(button::subtle),
        ]
        .align_y(iced::Center)
        .padding([10, 12]),
        scrollable(recents_list).height(Fill),
    ])
    .width(Fill)
    .height(Fill)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().background.weak.color)),
        ..container::Style::default()
    });
    let main_content: Element<'a, Message> = if wide {
        row![actions, recent_panel].spacing(1).into()
    } else {
        // Narrow window: show both sections without overflowing the viewport.
        column![
            container(actions).height(Length::FillPortion(2)),
            container(recent_panel).height(Length::FillPortion(1))
        ]
        .into()
    };
    let panel = container(main_content)
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .style(|theme: &Theme| {
            let p = theme.palette();
            container::Style {
                background: Some(Background::Color(p.background.base.color)),
                border: Border {
                    color: p.background.strong.color,
                    width: 1.0,
                    radius: 7.0.into(),
                },
                ..container::Style::default()
            }
        });
    let scrim = mouse_area(
        container(Space::new().width(Fill).height(Fill))
            .width(Fill)
            .height(Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.52))),
                ..container::Style::default()
            })
    )
    .on_press(Message::AppFileMenuClose);
    let positioned = column![
        row![iced::widget::opaque(panel), Space::new().width(Fill)],
        Space::new().height(Fill),
    ]
    .width(Fill)
    .height(Fill);
    iced::widget::stack![scrim, positioned].width(Fill).height(Fill).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_menu_actions_route_to_existing_commands() {
        let commands: Vec<&str> = ACTIONS.iter().map(|entry| entry.1).collect();
        assert!(commands.contains(&"OPEN"));
        assert!(commands.contains(&"QSAVE"));
        assert!(commands.contains(&"SAVEALL"));
        assert!(commands.contains(&"PLOT"));
        assert!(commands.contains(&"QUIT"));
        assert_eq!(commands.len(), 16);
    }
}
