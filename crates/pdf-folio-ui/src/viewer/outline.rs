//! Viewer outline/sidebar and jump dialog rendering.

use crate::*;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Element, Length};
use pdf_folio_core::OutlineNode;
use std::collections::HashSet;

pub(crate) fn view_sidebar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let body: Element<'_, Message> = if app.outline.is_empty() {
        container(
            text("No table of contents")
                .size(FontSize::MD)
                .color(tokens.text_secondary),
        )
        .padding(Spacing::LG)
        .width(Length::Fill)
        .into()
    } else {
        scrollable(outline_list(
            &app.outline,
            0,
            Vec::new(),
            &app.expanded_outline_paths,
            tokens,
        ))
        .direction(sidebar_scroll_direction())
        .height(Length::Fill)
        .style(move |_, status| sidebar_scrollable_style(tokens, status))
        .into()
    };

    container(
        column![section_heading("Contents", tokens), body]
            .spacing(Spacing::SM)
            .padding(Spacing::MD),
    )
    .width(app.layout().viewer_sidebar_width)
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::Sidebar))
    .into()
}

fn sidebar_scroll_direction() -> Direction {
    Direction::Vertical(
        Scrollbar::new()
            .width(4.0)
            .scroller_width(2.0)
            .anchor(Anchor::End),
    )
}

fn outline_list<'a>(
    nodes: &'a [OutlineNode],
    depth: u16,
    parent_path: Vec<usize>,
    expanded_paths: &'a HashSet<Vec<usize>>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut list = column![].spacing(Spacing::XS);

    for (index, node) in nodes.iter().enumerate() {
        let mut path = parent_path.clone();
        path.push(index);
        let has_children = !node.children.is_empty();
        let is_expanded = expanded_paths.contains(&path);
        let label = if node.title.trim().is_empty() {
            String::from("Untitled")
        } else {
            node.title.clone()
        };
        let mut row = row![
            text(" ".repeat(usize::from(depth) * 2)),
            text(if has_children {
                if is_expanded {
                    "v"
                } else {
                    ">"
                }
            } else {
                " "
            })
            .size(FontSize::SM)
            .color(tokens.text_secondary),
            text(label).size(FontSize::MD).color(tokens.text_primary)
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center);
        if let Some(page) = node.page {
            row = row.push(
                text(format!("{}", u32::from(page) + 1))
                    .size(FontSize::SM)
                    .color(tokens.text_secondary),
            );
            let message = if has_children {
                Message::ToggleOutlineNode(path.clone())
            } else {
                Message::JumpToPage(page)
            };
            list = list.push(outline_button(row, message, tokens));
        } else {
            list = list.push(outline_button(
                row,
                Message::ToggleOutlineNode(path.clone()),
                tokens,
            ));
        }

        if has_children && is_expanded {
            list = list.push(outline_list(
                &node.children,
                depth.saturating_add(1),
                path,
                expanded_paths,
                tokens,
            ));
        }
    }

    list.into()
}

fn outline_button<'a>(
    content: impl Into<Element<'a, Message>>,
    message: Message,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    toc_entry(content, tokens).on_press(message)
}

pub(crate) fn view_jump_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let max_page = app.doc.as_ref().map_or(0, |doc| doc.page_count());
    let dialog = row![
        text("Go to page")
            .size(FontSize::CONTROL)
            .color(tokens.text_primary),
        text_input("Page", &app.jump_input)
            .on_input(Message::JumpInputChanged)
            .on_submit(Message::SubmitJump)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(app.layout().jump_input_width),
        text(format!("of {max_page}"))
            .size(FontSize::MD)
            .color(tokens.text_secondary),
        toolbar_button("Go", tokens).on_press(Message::SubmitJump),
        toolbar_button("Cancel", tokens).on_press(Message::CloseOverlay),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::MD)
    .align_y(iced::Alignment::Center);

    container(dialog)
        .width(Length::Fill)
        .style(move |_| container_style(tokens, Class::JumpOverlay))
        .into()
}
