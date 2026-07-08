//! Selection and manual-reorder helpers for the library UI.

use std::collections::HashSet;

use pdf_folio_db::EntryId;

use pdf_folio_style::MasterCheckboxState;

pub fn range_selection_ids(
    first_index: usize,
    second_index: usize,
    entry_ids: &[EntryId],
) -> Vec<EntryId> {
    let start = first_index.min(second_index);
    let end = first_index
        .max(second_index)
        .min(entry_ids.len().saturating_sub(1));
    entry_ids[start..=end].to_vec()
}

pub fn toggle_selection_entry_id(selection: &mut HashSet<EntryId>, entry_id: EntryId) {
    if !selection.insert(entry_id.clone()) {
        selection.remove(&entry_id);
    }
}

pub fn master_checkbox_state_for_counts(
    selected_visible: usize,
    visible_count: usize,
) -> MasterCheckboxState {
    match selected_visible {
        0 => MasterCheckboxState::None,
        count if count == visible_count && visible_count > 0 => MasterCheckboxState::All,
        _ => MasterCheckboxState::Partial,
    }
}

pub fn reorder_entry_ids_for_drag(
    entries: &[EntryId],
    dragged_entries: &[EntryId],
    drop_index: usize,
) -> Vec<EntryId> {
    let dragged = dragged_entries.iter().cloned().collect::<HashSet<_>>();
    if dragged.is_empty() {
        return entries.to_vec();
    }

    let moving = entries
        .iter()
        .filter(|entry_id| dragged.contains(*entry_id))
        .cloned()
        .collect::<Vec<_>>();
    if moving.is_empty() {
        return entries.to_vec();
    }

    let mut remaining = entries
        .iter()
        .filter(|entry_id| !dragged.contains(*entry_id))
        .cloned()
        .collect::<Vec<_>>();
    let insert_index = drop_index.min(remaining.len());
    remaining.splice(insert_index..insert_index, moving);
    remaining
}

pub fn dragged_placeholder_count(entries: &[EntryId], dragged_entries: &[EntryId]) -> usize {
    let dragged = dragged_entries.iter().collect::<HashSet<_>>();
    entries
        .iter()
        .filter(|entry_id| dragged.contains(entry_id))
        .count()
}
