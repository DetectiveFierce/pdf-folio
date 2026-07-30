use crate::components::shared::error_banner::dismissible_error_banner;
use crate::components::viewer::page_controls::view_jump_dialog;
use crate::components::viewer::sidebar::view_sidebar;
use crate::components::viewer::toolbar::view_viewer_toolbar;
use crate::viewer::view::document::view_viewer_document;
use crate::*;
use iced::widget::{column, row};

pub(crate) fn view_viewer(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let sidebar: Element<'_, Message> = if app.viewer.toc_open {
        view_sidebar(app).into()
    } else {
        container("").width(Length::Shrink).into()
    };

    let mut main = column![].spacing(0);
    if let Some(error) = app.viewer.document_error.as_deref() {
        main = main.push(dismissible_error_banner(
            error,
            tokens,
            app.layout(),
            Message::DismissDocumentError,
        ));
    }
    if app.viewer.jump_dialog_open {
        main = main.push(view_jump_dialog(app));
    }
    main = main.push(view_viewer_document(app, tokens));

    column![
        view_viewer_toolbar(app),
        row![sidebar, main.width(Length::Fill)].height(Length::Fill)
    ]
    .into()
}
