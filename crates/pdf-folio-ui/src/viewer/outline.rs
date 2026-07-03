//! Viewer outline/sidebar and jump dialog rendering.

use crate::*;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::{button, column, container, image, row, scrollable, text, text_input};
use iced::{ContentFit, Element, Length};
use pdf_folio_core::{OutlineNode, TileKey};
use std::collections::HashSet;

pub(crate) fn view_sidebar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let heading = row![
        row![
            viewer_sidebar_tab_button(app, ViewerSidebarTab::Contents, tokens),
            viewer_sidebar_tab_button(app, ViewerSidebarTab::Thumbnails, tokens),
        ]
        .spacing(Spacing::XS)
        .width(Length::Fill),
        crate::library::view::sidebar_chevron_button(
            CHEVRON_LEFT_SVG,
            "Hide Contents",
            Message::ToggleSidebar,
            tokens,
        ),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);
    let body = match app.viewer.viewer_sidebar_tab {
        ViewerSidebarTab::Contents => view_outline_body(app, tokens),
        ViewerSidebarTab::Thumbnails => view_thumbnails_body(app, tokens),
    };

    container(
        column![heading, body]
            .spacing(Spacing::SM)
            .padding(Spacing::MD),
    )
    .width(app.layout().viewer_sidebar_width)
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::ViewerSidebar))
    .into()
}

fn viewer_sidebar_tab_button<'a>(
    app: &PDFolioApp,
    tab: ViewerSidebarTab,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    let active = app.viewer.viewer_sidebar_tab == tab;
    button(
        text(tab.label())
            .size(FontSize::SM)
            .font(ui_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(if active {
                tokens.text_primary
            } else {
                tokens.text_secondary
            })
            .wrapping(Wrapping::None),
    )
    .padding([Spacing::XS, Spacing::SM])
    .style(move |_, status| {
        if active {
            let active_style = tokens.class_styles[Class::ViewerSidebarTab.index()]
                .resolve(ComponentState::Active);
            crate::style::button_style(tokens, Class::ViewerSidebarTab, status)
                .with_visual_override(active_style)
        } else {
            crate::style::button_style(tokens, Class::ViewerSidebarTab, status)
        }
    })
    .on_press(Message::ViewerSidebarTabSelected(tab))
}

fn view_outline_body(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    if app.viewer.outline.is_empty() {
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
            &app.viewer.outline,
            0,
            Vec::new(),
            &app.viewer.expanded_outline_paths,
            tokens,
        ))
        .direction(sidebar_scroll_direction())
        .height(Length::Fill)
        .style(move |_, status| sidebar_scrollable_style(tokens, status))
        .into()
    }
}

fn view_thumbnails_body(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let Some(doc) = app.viewer.doc.as_ref() else {
        return container("").height(Length::Fill).into();
    };

    let mut pages = column![]
        .spacing(Spacing::MD)
        .padding([Spacing::SM, 0.0])
        .width(Length::Fill)
        .align_x(iced::Alignment::Center);
    for page in 0..doc.page_count() {
        pages = pages.push(thumbnail_button(app, page, tokens));
    }

    scrollable(pages)
        .direction(sidebar_scroll_direction())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_, status| sidebar_scrollable_style(tokens, status))
        .into()
}

fn thumbnail_button(app: &PDFolioApp, page: u16, tokens: ThemeTokens) -> Element<'_, Message> {
    let width = f32::from(app.layout().viewer_thumbnail_width_px);
    let height = width * app.viewer.page_aspect_ratios[usize::from(page)];
    let key = TileKey {
        page,
        width_px: app.layout().viewer_thumbnail_width_px,
    };

    let preview: Element<'_, Message> = if let Some(rendered) = app.viewer.rendered_pages.get(&key)
    {
        let image_height = width * f32::from(rendered.height) / f32::from(rendered.width.max(1));
        container(
            image(rendered.handle.clone())
                .width(Length::Fixed(width))
                .height(Length::Fixed(image_height))
                .content_fit(ContentFit::Contain),
        )
        .width(Length::Fixed(width))
        .height(Length::Fixed(image_height))
        .clip(true)
        .style(move |_| container_style(tokens, Class::ViewerPagePlaceholder))
        .into()
    } else {
        container(
            pdf_folio_ui_components::library::view::document_preview_lines(
                width, height, tokens, 0.82,
            ),
        )
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .style(move |_| container_style(tokens, Class::ViewerPagePlaceholder))
        .into()
    };

    let active = app.current_page() == page;
    let content = column![
        preview,
        text(format!("Page {}", u32::from(page) + 1))
            .size(FontSize::SM)
            .font(ui_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(if active {
                tokens.text_primary
            } else {
                tokens.text_secondary
            })
            .wrapping(Wrapping::None)
    ]
    .spacing(Spacing::XS)
    .width(Length::Fixed(width))
    .align_x(iced::Alignment::Center);

    button(content)
        .width(Length::Shrink)
        .padding(Spacing::SM)
        .style(move |_, status| {
            crate::style::button_style(tokens, Class::ViewerOutlineEntry, status)
        })
        .on_press(Message::JumpToPage(page))
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
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let max_page = app.viewer.doc.as_ref().map_or(0, |doc| doc.page_count());
    let dialog = row![
        text("Go to page")
            .size(FontSize::CONTROL)
            .color(tokens.text_primary),
        text_input("Page", &app.viewer.jump_input)
            .on_input(Message::JumpInputChanged)
            .on_submit(Message::SubmitJump)
            .style(move |_, status| text_input_style(tokens, Class::ViewerFindInput, status))
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
