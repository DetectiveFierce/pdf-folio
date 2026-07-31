//! # Viewer sidebar shell
//!
//! Host container under `components::viewer::sidebar` for the open document’s
//! secondary pane: Contents (outline) and Thumbnails tabs, with a hide
//! control. Bodies scroll independently of the main canvas.
//!
//! ## Ownership
//!
//! Reads `app.viewer.viewer_sidebar_tab`, outline expansion, and rendered
//! thumbnail tiles. Emits tab selection, outline toggles, page jumps, and
//! `ToggleSidebar`. Outline rows come from [`super::outline`]; thumbnails
//! reuse placeholder previews from `components::library::cards`.
//!
//! Related: floating “show contents” control in [`super::toolbar`].

use crate::components::viewer::outline::outline_list;
use crate::*;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::{button, column, image, row, scrollable};
use iced::ContentFit;
use pdf_folio_core::TileKey;

/// Compose the full viewer sidebar (tab strip + Contents or Thumbnails body).
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

/// Contents / Thumbnails tab button with active styling when selected.
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

/// Scrollable outline list, or an empty-state message when the PDF has no TOC.
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
            app.current_page(),
            tokens,
        ))
        .direction(sidebar_scroll_direction(tokens))
        .height(Length::Fill)
        .style(move |_, status| sidebar_scrollable_style(tokens, status))
        .into()
    }
}

/// Scrollable column of page thumbnail buttons for the Thumbnails tab.
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
        .direction(sidebar_scroll_direction(tokens))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_, status| sidebar_scrollable_style(tokens, status))
        .into()
}

/// One page thumbnail (rendered tile or placeholder) that jumps to `page` on press.
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
        container(crate::components::library::cards::document_preview_lines(
            width, height, tokens, 0.82,
        ))
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

/// Vertical scrollbar configuration for viewer sidebar panes.
fn sidebar_scroll_direction(tokens: ThemeTokens) -> Direction {
    Direction::Vertical(
        Scrollbar::new()
            .width(tokens.primitives.sidebar_scrollbar_width)
            .scroller_width(tokens.primitives.sidebar_scrollbar_scroller_width)
            .anchor(Anchor::End),
    )
}
