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
/// `expanded_paths`. Nodes with children toggle expansion; leaf nodes with a
/// page target jump there.
pub(crate) fn outline_list<'a>(
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

/// Pressable TOC entry styled with `toc_entry`, emitting `message` on activate.
fn outline_button<'a>(
    content: impl Into<Element<'a, Message>>,
    message: Message,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    toc_entry(content, tokens).on_press(message)
}
