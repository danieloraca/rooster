mod rerank;
pub mod session;
use rooster_core::artifacts::ScopeAssessment;
use session::{DesktopInventory, Inspection, ScanRequest, Session, Settings, Status};
use std::sync::Arc;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

type AppSession = Arc<Session>;

#[tauri::command]
fn load_settings(state: State<'_, AppSession>) -> Result<Settings, String> {
    state.settings()
}

#[tauri::command]
async fn choose_workspace(
    app: tauri::AppHandle,
    state: State<'_, AppSession>,
    name: String,
) -> Result<Option<Settings>, String> {
    if name.trim().is_empty() {
        return Err("Enter a workspace name first.".into());
    }
    let session = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let selected = app
            .dialog()
            .file()
            .set_title("Add a workspace folder")
            .blocking_pick_folder();
        selected
            .map(|selection| {
                let path = selection
                    .into_path()
                    .map_err(|_| "Choose a local folder.")?;
                session.register_selection(&name, &path)
            })
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn remove_location(
    state: State<'_, AppSession>,
    root_id: String,
) -> Result<Settings, String> {
    let session = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || session.remove_location(&root_id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn start_scan(state: State<'_, AppSession>, request: ScanRequest) -> Result<u64, String> {
    state.inner().start(request)
}
#[tauri::command]
fn scan_status(state: State<'_, AppSession>) -> Result<Status, String> {
    state.status()
}
#[tauri::command]
fn cancel_scan(state: State<'_, AppSession>, ticket: u64) -> Result<(), String> {
    state.cancel(ticket)
}
#[tauri::command]
fn get_inventory(
    state: State<'_, AppSession>,
    generation: u64,
) -> Result<DesktopInventory, String> {
    state.inventory(generation)
}
#[tauri::command]
fn inspect_artifact(
    state: State<'_, AppSession>,
    generation: u64,
    id: String,
) -> Result<Inspection, String> {
    state.inspect(generation, &id)
}
#[tauri::command]
fn assess_context(
    state: State<'_, AppSession>,
    generation: u64,
    id: String,
) -> Result<ScopeAssessment, String> {
    state.assess(generation, &id)
}
#[tauri::command]
async fn search_artifacts(
    state: State<'_, AppSession>,
    generation: u64,
    query: String,
) -> Result<Vec<String>, String> {
    let session = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || session.search(generation, &query))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn editing(
    state: State<'_, AppSession>,
    request: session::EditingRequest,
) -> Result<serde_json::Value, String> {
    let session = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || session.editing(request))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn choose_draft_source(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let Some(selected) = app
            .dialog()
            .file()
            .set_title("Import text into this draft")
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = selected
            .into_path()
            .map_err(|_| "Choose a local text file")?;
        if !std::fs::metadata(&path)
            .map_err(|e| e.to_string())?
            .is_file()
        {
            return Err("Choose a regular text file".into());
        }
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut bytes = vec![];
        file.take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("Source exceeds 16 MiB".into());
        }
        Ok(Some(
            String::from_utf8(bytes).map_err(|_| "Choose a UTF-8 text file")?,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let session = Session::from_environment().map_err(std::io::Error::other)?;
            app.manage(Arc::new(session));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            choose_workspace,
            remove_location,
            start_scan,
            scan_status,
            cancel_scan,
            get_inventory,
            inspect_artifact,
            assess_context,
            search_artifacts,
            editing,
            choose_draft_source
        ])
        .run(tauri::generate_context!())
        .expect("Rooster desktop failed");
}
