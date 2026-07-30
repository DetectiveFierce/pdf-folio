//! # Library entry cards and rows
//!
//! Builds individual PDF tiles for grid (card) and list (row) layouts,
//! including selection chrome, drag ghost styling, and thumbnail media.
//!
//! Domain-aware: uses app selection, drag state, and thumbnail cache.
//! Low-level drop-zone and tag row widgets come from `components::library::cards`.

use super::*;
use iced::widget::column;

/// Grid-mode card for one library entry (thumbnail, title, meta, selection).
///
/// Builds media + info chrome at the current grid zoom, applies hover lift,
/// and wires drag / context-menu / hover messages when `mode` is Normal.
/// Placeholder and Floating modes omit interaction handlers and use ghost alpha.
pub(crate) fn library_entry_card<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
    mode: LibraryEntryRenderMode,
) -> Element<'a, Message> {
    let entry_id = entry.id.clone();
    let selected = app.library.selected_library_entries.contains(&entry_id);
    let title = entry_title(&entry);
    let tags = entry.tags.iter().take(3).cloned().collect::<Vec<_>>();
    let author = entry
        .display_author
        .clone()
        .or_else(|| entry.author.clone())
        .unwrap_or_else(|| String::from("Unknown author"));
    let metadata_label = library_card_metadata_label(app.library.library_metadata_density, &entry);
    let search_match = library_search_match_label(app, &entry, &entry_id);
    let content_alpha = library_entry_content_alpha(app, mode);
    let text_secondary = with_alpha(tokens.text_secondary, content_alpha);
    let accent = with_alpha(tokens.accent, content_alpha);
    let progress_value = progress_fraction(&entry);
    let media = card_thumbnail_media(app, &entry_id, tokens, content_alpha);
    let title_font_size = app.library_card_title_font_size();
    let metadata_font_size = app.library_card_font_size(FontSize::SM);
    let text_width = app.library_card_title_width();
    let author = truncate_for_width_with_font(&author, text_width, 0.0, metadata_font_size);
    let metadata_label = metadata_label
        .map(|label| truncate_for_width_with_font(&label, text_width, 0.0, metadata_font_size));
    let search_match = search_match
        .map(|label| truncate_for_width_with_font(&label, text_width, 0.0, metadata_font_size));
    let hover_progress = if mode == LibraryEntryRenderMode::Normal {
        app.library_card_hover_progress(&entry_id)
    } else {
        0.0
    };
    let top_lift_space = app.library_card_hover_lift() * (1.0 - hover_progress);
    let bottom_lift_space = app.library_card_hover_lift() * hover_progress;
    let mut info = column![truncated_title(
        title,
        text_width,
        tokens,
        content_alpha,
        title_font_size
    ),]
    .spacing(app.library_card_spacing())
    .padding(app.library_card_padding())
    .height(app.library_card_info_height())
    .width(Length::Fill);
    if !tags.is_empty() {
        let tag_row_left_shift =
            app.layout()
                .metric("LibraryEntry", "card_tag_row_left_shift", -4.0);
        info = info.push(if mode != LibraryEntryRenderMode::Normal {
            compact_tags_row(tags, tokens, content_alpha, tag_row_left_shift, None)
        } else {
            compact_tags_row(
                tags,
                tokens,
                content_alpha,
                tag_row_left_shift,
                Some(Message::TagPillClicked),
            )
        });
    }
    info = info.push(
        text(author)
            .size(metadata_font_size)
            .font(ui_font(FontWeight::REGULAR))
            .color(text_secondary)
            .wrapping(Wrapping::None),
    );
    if let Some(metadata_label) = metadata_label {
        info = info.push(
            text(metadata_label)
                .size(metadata_font_size)
                .font(ui_font(FontWeight::REGULAR))
                .color(text_secondary)
                .wrapping(Wrapping::None),
        );
    }
    if let Some(search_match) = search_match {
        info = info.push(
            text(search_match)
                .size(metadata_font_size)
                .font(ui_font(FontWeight::MEDIUM))
                .color(accent)
                .wrapping(Wrapping::None),
        );
    }
    info = info.push(progress_bar(progress_value, tokens));

    if mode == LibraryEntryRenderMode::Normal
        && app.library.tag_entry_id.as_ref() == Some(&entry_id)
    {
        info = info.push(
            text_input("Tag", &app.library.tag_input)
                .on_input(Message::TagInputChanged)
                .on_submit(Message::SubmitTag),
        );
    }
    let checkbox_visible = selected
        || !app.library.selected_library_entries.is_empty()
        || app.library_card_hover_progress(&entry_id) > 0.01;
    let media = if mode == LibraryEntryRenderMode::Normal && checkbox_visible {
        stack![
            media,
            container(selection_checkbox(
                selected,
                tokens,
                Message::EntryCheckboxToggled(entry_id.clone())
            ))
            .padding(Spacing::SM)
            .width(Length::Shrink)
            .height(Length::Shrink),
        ]
        .into()
    } else {
        media
    };
    let body = column![media, info].spacing(0).width(Length::Fill);
    let width = Length::Fixed(app.library_grid_card_width());
    let surface = container(body).width(width).clip(true).style(move |_| {
        library_entry_container_style(tokens, Class::LibraryCard, mode, selected, hover_progress)
    });
    let lifted_surface = column![
        container("").height(top_lift_space),
        surface,
        container("").height(bottom_lift_space),
    ]
    .spacing(0)
    .width(width);

    if mode != LibraryEntryRenderMode::Normal {
        lifted_surface.into()
    } else {
        let area = mouse_area(lifted_surface)
            .on_enter(Message::LibraryEntryHoverChanged(entry_id.clone(), true))
            .on_exit(Message::LibraryEntryHoverChanged(entry_id.clone(), false))
            .on_right_press(Message::ContextMenuOpened(ContextMenuTarget::LibraryEntry(
                entry_id.clone(),
            )))
            .on_press(Message::BeginLibraryEntryDrag(entry_id.clone()))
            .on_release(Message::EndLibraryEntryDrag);
        if app
            .library
            .library_drag
            .as_ref()
            .is_some_and(|drag| drag.active)
        {
            area.interaction(mouse::Interaction::Grabbing).into()
        } else {
            area.into()
        }
    }
}

/// Compact list-mode row for one library entry (checkbox, thumb, meta, progress).
///
/// Same interaction wiring as [`library_entry_card`] but uses list layout tokens
/// (fixed thumbnail/progress widths). Floating mode uses a fixed preview width.
pub(crate) fn library_entry_row<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
    mode: LibraryEntryRenderMode,
) -> Element<'a, Message> {
    let entry_id = entry.id.clone();
    let selected = app.library.selected_library_entries.contains(&entry_id);
    let title = entry_title(&entry);
    let details = library_row_metadata_label(app.library.library_metadata_density, &entry);
    let tags = entry.tags.clone();
    let progress_value = progress_fraction(&entry);
    let search_match = library_search_match_label(app, &entry, &entry_id);
    let content_alpha = library_entry_content_alpha(app, mode);
    let hover_progress = if mode == LibraryEntryRenderMode::Normal {
        app.library_card_hover_progress(&entry_id)
    } else {
        0.0
    };
    let top_lift_space = app.library_row_hover_lift() * (1.0 - hover_progress);
    let bottom_lift_space = app.library_row_hover_lift() * hover_progress;
    let text_secondary = with_alpha(tokens.text_secondary, content_alpha);
    let accent = with_alpha(tokens.accent, content_alpha);
    let mut detail_column = column![
        truncated_title(
            title,
            app.layout().library_row_title_width,
            tokens,
            content_alpha,
            16
        ),
        text(details)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(text_secondary),
    ]
    .spacing(Spacing::XS)
    .width(Length::Fill);
    if let Some(match_label) = search_match {
        detail_column = detail_column.push(
            text(match_label)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(accent),
        );
    }
    detail_column = detail_column.push(if mode != LibraryEntryRenderMode::Normal {
        ghost_tags_row(tags, tokens, content_alpha)
    } else {
        component_tags_row(
            tags,
            tokens,
            Message::TagPillClicked,
            Message::StartTagEntry(entry_id.clone()),
        )
    });
    let checkbox_lane: Element<'a, Message> = if mode == LibraryEntryRenderMode::Normal
        && (selected
            || !app.library.selected_library_entries.is_empty()
            || app.library_card_hover_progress(&entry_id) > 0.01)
    {
        selection_checkbox(
            selected,
            tokens,
            Message::EntryCheckboxToggled(entry_id.clone()),
        )
        .into()
    } else {
        container("")
            .width(Length::Fixed(app.layout().metric(
                "LibraryEntry",
                "row_checkbox_lane_width",
                24.0,
            )))
            .into()
    };
    let row_content = row![
        checkbox_lane,
        thumbnail_element(
            app,
            &entry_id,
            tokens,
            app.layout().library_row_thumbnail_width,
            content_alpha
        ),
        detail_column,
        column![progress_bar(progress_value, tokens),]
            .spacing(Spacing::XS)
            .width(app.layout().library_row_progress_width),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::SM)
    .align_y(iced::Alignment::Center);

    let width = if mode == LibraryEntryRenderMode::Floating {
        Length::Fixed(
            app.layout()
                .metric("LibraryEntry", "row_floating_width", 720.0),
        )
    } else {
        Length::Fill
    };
    let surface = container(row_content).width(width).style(move |_| {
        library_entry_container_style(tokens, Class::LibraryRow, mode, selected, hover_progress)
    });
    let lifted_surface = column![
        container("").height(top_lift_space),
        surface,
        container("").height(bottom_lift_space),
    ]
    .spacing(0)
    .width(width);

    if mode != LibraryEntryRenderMode::Normal {
        lifted_surface.into()
    } else {
        let area = mouse_area(lifted_surface)
            .on_enter(Message::LibraryEntryHoverChanged(entry_id.clone(), true))
            .on_exit(Message::LibraryEntryHoverChanged(entry_id.clone(), false))
            .on_right_press(Message::ContextMenuOpened(ContextMenuTarget::LibraryEntry(
                entry_id.clone(),
            )))
            .on_press(Message::BeginLibraryEntryDrag(entry_id.clone()))
            .on_release(Message::EndLibraryEntryDrag);
        if app
            .library
            .library_drag
            .as_ref()
            .is_some_and(|drag| drag.active)
        {
            area.interaction(mouse::Interaction::Grabbing).into()
        } else {
            area.into()
        }
    }
}

/// Horizontal tag chips for compact card/row density; optional press handler per tag.
fn compact_tags_row<'a>(
    tags: Vec<String>,
    tokens: ThemeTokens,
    alpha: f32,
    left_shift: f32,
    on_tag: Option<fn(String) -> Message>,
) -> Element<'a, Message> {
    let mut row = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
    for tag in tags {
        let pill: Element<'a, Message> = if let Some(on_tag) = on_tag {
            tag_pill(tag.clone(), tokens).on_press(on_tag(tag)).into()
        } else {
            container(
                text(tag)
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(with_alpha(tokens.text_secondary, alpha)),
            )
            .padding([Spacing::XS, Spacing::MD])
            .style(move |_| {
                let mut style = container_style(tokens, Class::TagPill);
                if let Some(iced::Background::Color(mut background)) = style.background {
                    background.a *= alpha;
                    style.background = Some(iced::Background::Color(background));
                }
                style.border.color = with_alpha(style.border.color, alpha);
                style
            })
            .into()
        };
        row = row.push(pill);
    }
    container(row)
        .padding(iced::Padding {
            left: left_shift,
            ..iced::Padding::ZERO
        })
        .into()
}

/// Container style for selected/hover/drag states on an entry tile.
///
/// Interpolates Normal↔Hovered visual tokens by `hover_progress` (`0..=1`),
/// then overlays Selected when checked. Placeholder uses Disabled chrome;
/// Floating uses Active (elevated) chrome. `class` is typically `LibraryCard`
/// or `LibraryRow`.
pub(crate) fn library_entry_container_style(
    tokens: ThemeTokens,
    class: Class,
    mode: LibraryEntryRenderMode,
    selected: bool,
    hover_progress: f32,
) -> iced::widget::container::Style {
    let mut style = container_style(tokens, class);
    match mode {
        LibraryEntryRenderMode::Normal => {
            let hover_progress = hover_progress.clamp(0.0, 1.0);
            let normal_style = tokens.class_styles[class.index()].resolve(ComponentState::Normal);
            let hovered_style = tokens.class_styles[class.index()].resolve(ComponentState::Hovered);
            let normal_background = normal_style
                .background
                .or_else(|| {
                    style.background.and_then(|background| match background {
                        iced::Background::Color(color) => Some(color),
                        _ => None,
                    })
                })
                .unwrap_or(tokens.surface_raised);
            let hovered_background = hovered_style
                .background
                .unwrap_or_else(|| mix_color(normal_background, tokens.accent, 0.14));
            let normal_border = normal_style.border_color.unwrap_or(style.border.color);
            let hovered_border = hovered_style
                .border_color
                .unwrap_or_else(|| mix_color(normal_border, tokens.accent, 0.42));

            if !selected && hover_progress > 0.0 {
                style.background = Some(iced::Background::Color(mix_color(
                    normal_background,
                    hovered_background,
                    hover_progress,
                )));
                style.border.color = mix_color(normal_border, hovered_border, hover_progress);
            }

            if let Some(shadow) =
                interpolate_shadow(normal_style.shadow, hovered_style.shadow, hover_progress)
            {
                style.shadow = shadow;
            }
            if selected {
                let selected_style =
                    tokens.class_styles[class.index()].resolve(ComponentState::Selected);
                if let Some(background) = selected_style.background {
                    style.background = Some(iced::Background::Color(background));
                }
                if let Some(border_color) = selected_style.border_color {
                    style.border.color = border_color;
                }
                if let Some(border_width) = selected_style.border_width {
                    style.border.width = border_width;
                }
                if let Some(shadow) =
                    interpolate_shadow(selected_style.shadow, hovered_style.shadow, hover_progress)
                {
                    style.shadow = shadow;
                }
            }
        }
        LibraryEntryRenderMode::Placeholder => {
            let placeholder_style =
                tokens.class_styles[class.index()].resolve(ComponentState::Disabled);
            style = style.with_visual_override(placeholder_style);
        }
        LibraryEntryRenderMode::Floating => {
            let floating_style = tokens.class_styles[class.index()].resolve(ComponentState::Active);
            style = style.with_visual_override(floating_style);
        }
    }
    style
}

/// Linearly blend two style-book shadows by `progress` in `0.0..=1.0` (for hover/lift).
fn interpolate_shadow(
    from: Option<crate::style::BoxShadow>,
    to: Option<crate::style::BoxShadow>,
    progress: f32,
) -> Option<iced::Shadow> {
    let from = from?;
    let to = to.unwrap_or(from);
    let progress = progress.clamp(0.0, 1.0);
    Some(iced::Shadow {
        color: mix_color(from.color, to.color, progress),
        offset: iced::Vector::new(
            from.offset_x + (to.offset_x - from.offset_x) * progress,
            from.offset_y + (to.offset_y - from.offset_y) * progress,
        ),
        blur_radius: from.blur_radius + (to.blur_radius - from.blur_radius) * progress,
    })
}

/// Opacity for entry content while dragging (`0..=1`; ghost vs solid).
///
/// Placeholder mode uses `library_drag_placeholder_content_alpha`; Normal and
/// Floating stay fully opaque.
pub(crate) fn library_entry_content_alpha(app: &PDFolioApp, mode: LibraryEntryRenderMode) -> f32 {
    if mode == LibraryEntryRenderMode::Placeholder {
        app.layout().library_drag_placeholder_content_alpha
    } else {
        1.0
    }
}

/// Thumbnail image region for a grid card, with document-preview lines when missing.
///
/// Width matches the current card width; height is aspect-scaled and capped by
/// `library_card_media_max_height`. Prefers the zoom-appropriate thumbnail tier.
pub(crate) fn card_thumbnail_media<'a>(
    app: &'a PDFolioApp,
    entry_id: &EntryId,
    tokens: ThemeTokens,
    alpha: f32,
) -> Element<'a, Message> {
    let width = app.library_grid_card_width();
    if let Some(thumbnail) = app.thumbnail_for_entry(entry_id, app.thumbnail_size_for_grid_zoom()) {
        let height = (width * f32::from(thumbnail.height) / f32::from(thumbnail.width.max(1)))
            .min(app.library_card_media_max_height());
        container(
            image(thumbnail.handle.clone())
                .width(width)
                .height(height)
                .content_fit(ContentFit::Cover)
                .border_radius(iced::border::bottom(crate::style::Radius::MD))
                .opacity(alpha),
        )
        .width(width)
        .height(height)
        .clip(true)
        .style(move |_| flush_media_style(tokens, alpha))
        .into()
    } else {
        container(document_preview_lines(
            width,
            app.library_card_media_max_height(),
            tokens,
            alpha,
        ))
        .center(width)
        .height(app.library_card_media_max_height())
        .style(move |_| flush_media_style(tokens, alpha))
        .into()
    }
}

/// Iced image element from a cached `ThumbnailView`, scaled to the target size.
///
/// `width` is logical px; height follows cover aspect up to
/// `thumbnail_max_height_ratio * width`. Falls back to a `"PDF"` placeholder
/// when no Default-tier thumb is cached. Used by list rows and detail panels.
pub(crate) fn thumbnail_element<'a>(
    app: &'a PDFolioApp,
    entry_id: &EntryId,
    tokens: ThemeTokens,
    width: f32,
    alpha: f32,
) -> Element<'a, Message> {
    let max_height = width
        * app
            .layout()
            .metric("LibraryEntry", "thumbnail_max_height_ratio", 1.32);
    if let Some(thumbnail) = app.thumbnail_for_entry(entry_id, ThumbnailSize::Default) {
        let height = width * f32::from(thumbnail.height) / f32::from(thumbnail.width.max(1));
        let display_height = height.min(max_height);
        container(
            image(thumbnail.handle.clone())
                .width(width)
                .height(height)
                .opacity(alpha),
        )
        .width(width)
        .height(display_height)
        .clip(true)
        .style(move |_| flush_media_style(tokens, alpha))
        .into()
    } else {
        container(
            text("PDF")
                .size(FontSize::SM)
                .color(with_alpha(tokens.text_secondary, alpha)),
        )
        .center(width)
        .height(max_height)
        .style(move |_| {
            let mut style = container_style(tokens, Class::PagePlaceholder);
            if alpha < 1.0 {
                if let Some(iced::Background::Color(mut background)) = style.background {
                    background.a *= alpha;
                    style.background = Some(iced::Background::Color(background));
                }
                style.border.color = with_alpha(style.border.color, alpha);
            }
            style
        })
        .into()
    }
}
