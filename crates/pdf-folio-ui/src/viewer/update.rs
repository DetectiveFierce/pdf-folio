use crate::*;

pub(crate) fn update(app: &mut PDFolioApp, message: &Message) -> Option<Task<Message>> {
    match message {
        Message::ToggleSidebar | Message::ToggleTocPanel => {
            app.viewer.toc_open = !app.viewer.toc_open;
            app.viewer.viewer_viewport_width = app.estimated_viewer_viewport_width();
            app.viewer.viewer_viewport_height = app.estimated_viewer_viewport_height();
            Some(with_session_save(app.apply_active_dimension_zoom(), app))
        }
        Message::ViewerSidebarTabSelected(tab) => {
            app.viewer.viewer_sidebar_tab = *tab;
            Some(with_session_save(app.request_viewer_thumbnail_pages(), app))
        }
        _ => None,
    }
}
