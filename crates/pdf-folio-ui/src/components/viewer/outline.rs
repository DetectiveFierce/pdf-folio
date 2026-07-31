//! # Document outline
//!
//! Hierarchical table-of-contents list under `components::viewer::outline`.
//! Renders nested outline nodes with expand/collapse paths and page jump
//! actions for the viewer sidebar Contents tab.
//!
//! ## Ownership
//!
//! Pure presentation given `OutlineNode` slices and an expanded-path set from
//! `app.viewer`. Emits `ToggleOutlineNode` or `JumpToPage`. Hosted by
//! [`super::sidebar`]; document outline data is loaded with the PDF in the
//! viewer domain.
//!
//! Related: thumbnail page jumps in the same sidebar; page chrome in
//! [`super::page_controls`].

use crate::*;
use iced::widget::{column, row};
use pdf_folio_core::OutlineNode;
use std::collections::HashSet;

/// Recursive interactive outline list for the Contents sidebar tab.
///
/// `depth` and `parent_path` drive indentation and stable node keys in
/// `expanded_paths`. Nodes with children expand via the chevron control; the
/// title row jumps when the node has a page target. `current_page` accents the
/// best matching entry for scroll-spy feedback.
pub(crate) fn outline_list<'a>(
    nodes: &'a [OutlineNode],
    depth: u16,
    parent_path: Vec<usize>,
    expanded_paths: &'a HashSet<Vec<usize>>,
    current_page: u16,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut list = column![].spacing(Spacing::XS);
    let active_path = active_outline_path(nodes, current_page);

    for (index, node) in nodes.iter().enumerate() {
        let mut path = parent_path.clone();
        path.push(index);
        let has_children = !node.children.is_empty();
        let is_expanded = expanded_paths.contains(&path);
        let is_current = active_path.as_ref() == Some(&path);
        let label = if node.title.trim().is_empty() {
            String::from("Untitled")
        } else {
            node.title.clone()
        };

        let expand_control: Element<'_, Message> = if has_children {
            let chevron = text(if is_expanded { "v" } else { ">" })
                .size(FontSize::SM)
                .color(tokens.text_secondary)
                .wrapping(Wrapping::None);
            button(chevron)
                .padding([Spacing::XS / 2.0, Spacing::XS])
                .style(move |_, status| {
                    crate::style::button_style(tokens, Class::ViewerOutlineEntry, status)
                })
                .on_press(Message::ToggleOutlineNode(path.clone()))
                .into()
        } else {
            text(" ")
                .size(FontSize::SM)
                .color(tokens.text_secondary)
                .into()
        };

        let mut title = text(label)
            .size(FontSize::MD)
            .color(if is_current {
                tokens.accent
            } else {
                tokens.text_primary
            })
            .wrapping(Wrapping::Word);
        if is_current {
            title = title.font(ui_font(FontWeight::SEMIBOLD));
        }

        let mut content = row![
            text(" ".repeat(usize::from(depth) * 2)),
            expand_control,
            title.width(Length::Fill),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center);

        if let Some(page) = node.page {
            content = content.push(
                text(format!("{}", u32::from(page) + 1))
                    .size(FontSize::SM)
                    .color(if is_current {
                        tokens.accent
                    } else {
                        tokens.text_secondary
                    }),
            );
        }

        let row_message = if let Some(page) = node.page {
            Message::JumpToPage(page)
        } else if has_children {
            Message::ToggleOutlineNode(path.clone())
        } else {
            Message::ToggleOutlineNode(path.clone())
        };

        list = list.push(outline_button(content, row_message, is_current, tokens));

        if has_children && is_expanded {
            list = list.push(outline_list(
                &node.children,
                depth.saturating_add(1),
                path,
                expanded_paths,
                current_page,
                tokens,
            ));
        }
    }

    list.into()
}

/// Deepest outline path whose page is ≤ `current_page` (scroll-spy selection).
fn active_outline_path(nodes: &[OutlineNode], current_page: u16) -> Option<Vec<usize>> {
    let mut best: Option<(u16, Vec<usize>)> = None;
    walk_outline_pages(nodes, &mut Vec::new(), current_page, &mut best);
    best.map(|(_, path)| path)
}

fn walk_outline_pages(
    nodes: &[OutlineNode],
    path: &mut Vec<usize>,
    current_page: u16,
    best: &mut Option<(u16, Vec<usize>)>,
) {
    for (index, node) in nodes.iter().enumerate() {
        path.push(index);
        if let Some(page) = node.page {
            if page <= current_page {
                let replace = best
                    .as_ref()
                    .map_or(true, |(best_page, _)| page >= *best_page);
                if replace {
                    *best = Some((page, path.clone()));
                }
            }
        }
        if !node.children.is_empty() {
            walk_outline_pages(&node.children, path, current_page, best);
        }
        path.pop();
    }
}

/// Pressable TOC entry styled with `toc_entry`, emitting `message` on activate.
fn outline_button<'a>(
    content: impl Into<Element<'a, Message>>,
    message: Message,
    active: bool,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    let mut button = toc_entry(content, tokens).on_press(message);
    if active {
        button = button.style(move |_, status| {
            let active_style = tokens.class_styles[Class::ViewerOutlineEntry.index()]
                .resolve(ComponentState::Active);
            crate::style::button_style(tokens, Class::ViewerOutlineEntry, status)
                .with_visual_override(active_style)
        });
    }
    button
}
