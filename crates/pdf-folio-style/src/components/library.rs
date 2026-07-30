use iced::widget::{button, container, text, Svg};
use iced::{Element, Length};

use crate::classes::{button_style, Class};
use crate::tokens::{ui_font, FontSize, FontWeight, ThemeTokens};

const CHECK_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>"##;

/// Creates a library card button from arbitrary content.
pub fn library_card<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(content)
        .width(Length::FillPortion(1))
        .style(move |_, status| button_style(tokens, Class::LibraryCard, status))
}

/// Creates a library list-row button from arbitrary content.
pub fn library_row<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(content)
        .width(Length::Fill)
        .style(move |_, status| button_style(tokens, Class::LibraryRow, status))
}

/// Selection state represented by the library master checkbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterCheckboxState {
    /// No visible entries are selected.
    None,
    /// Some, but not all, visible entries are selected.
    Partial,
    /// Every visible entry is selected.
    All,
}

/// Creates a library entry selection checkbox.
pub fn selection_checkbox<'a, Message: Clone + 'a>(
    checked: bool,
    tokens: ThemeTokens,
    on_toggle: Message,
) -> iced::widget::Button<'a, Message> {
    checkbox_button(
        selection_checkbox_mark(checked, tokens),
        tokens,
        Class::SelectionCheckbox,
    )
    .on_press(on_toggle)
}

/// Creates a master selection checkbox for all visible library entries.
pub fn master_checkbox<'a, Message: Clone + 'a>(
    state: MasterCheckboxState,
    tokens: ThemeTokens,
    on_click: Message,
) -> iced::widget::Button<'a, Message> {
    let label = match state {
        MasterCheckboxState::None => "",
        MasterCheckboxState::Partial => "−",
        MasterCheckboxState::All => "✓",
    };
    checkbox_button(
        checkbox_text_mark(label, tokens, FontSize::MD),
        tokens,
        Class::MasterCheckbox,
    )
    .on_press(on_click)
}

fn checkbox_button<'a, Message: Clone + 'a>(
    content: Element<'a, Message>,
    tokens: ThemeTokens,
    class: Class,
) -> iced::widget::Button<'a, Message> {
    let layout = tokens.class_styles[class.index()].layout;
    button(container(content).center(Length::Fill))
        .width(Length::Fixed(layout.width.unwrap_or(24.0)))
        .height(Length::Fixed(layout.height.unwrap_or(24.0)))
        .padding(layout.padding_x(0.0))
        .style(move |_, status| button_style(tokens, class, status))
}

fn selection_checkbox_mark<'a, Message: 'a>(
    checked: bool,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    if checked {
        let mark_size = tokens.class_styles[Class::SelectionCheckbox.index()]
            .layout
            .width
            .unwrap_or(24.0)
            * 0.75;
        Svg::new(iced::widget::svg::Handle::from_memory(CHECK_SVG))
            .width(mark_size)
            .height(mark_size)
            .style(move |_, _| iced::widget::svg::Style {
                color: Some(tokens.text_primary),
            })
            .into()
    } else {
        let mark_size = tokens.class_styles[Class::SelectionCheckbox.index()]
            .layout
            .width
            .unwrap_or(24.0)
            * 0.75;
        container("").width(mark_size).height(mark_size).into()
    }
}

fn checkbox_text_mark<'a, Message: 'a>(
    label: &'static str,
    tokens: ThemeTokens,
    mark_size: u32,
) -> Element<'a, Message> {
    text(label)
        .size(mark_size)
        .font(ui_font(FontWeight::BOLD))
        .color(tokens.text_primary)
        .align_x(iced::alignment::Horizontal::Center)
        .into()
}
