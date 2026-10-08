//! A read game's data saved for a spreadsheet: the window asks where, and the board writes it.
//!
//! The system's save dialog is opened from here rather than by the page, so the window holds no
//! permission to choose or write a path: it can only ask for a game's data, and the person picks
//! where it goes.

use tauri::{AppHandle, Manager};

use super::{cockpit::file_safe, library_dir, work};

/// Asks where to put a game's data, then puts writing it on the board. None where the person
/// closed the dialog without choosing.
#[tauri::command]
pub async fn export_data(app: AppHandle, app_id: u32) -> Result<Option<u64>, String> {
    let name = steamgauge_core::report::crawl_facts(&library_dir(&app), app_id)
        .map_or_else(|_| format!("App {app_id}"), |facts| facts.title());
    let mut asking = rfd::AsyncFileDialog::new()
        .set_title("Save the data in a new folder")
        .set_file_name(format!("{} - SteamGauge data", file_safe(&name)));
    if let Some(window) = app.get_webview_window("main") {
        asking = asking.set_parent(&window);
    }
    let Some(chosen) = asking.save_file().await else {
        return Ok(None);
    };
    let to = chosen.path().to_path_buf();
    if to.exists() {
        return Err(format!(
            "{} is there already; choose a name nothing has yet",
            to.display()
        ));
    }
    Ok(Some(app.state::<work::Work>().queue(
        &app,
        work::Task::ExportData { app_id, to },
        None,
    )))
}
