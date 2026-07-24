#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod crypto;
mod api;

use serde_json::Value;
use std::sync::Mutex;
use tauri::Manager;
use api::{AdminClient, ProviderModel, fetch_provider_models};
use crypto::KeyPair;

struct AppState {
    keypair: Mutex<Option<KeyPair>>,
}

#[tauri::command]
async fn get_service_status(state: tauri::State<'_, AppState>, server_url: String) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.get_service_status().await
}

#[tauri::command]
async fn refresh_cache(state: tauri::State<'_, AppState>, server_url: String) -> Result<(), String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.refresh_cache().await
}

#[tauri::command]
async fn fetch_providers(state: tauri::State<'_, AppState>, server_url: String) -> Result<Vec<Value>, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.list_providers().await
}

#[tauri::command]
async fn create_provider(state: tauri::State<'_, AppState>, server_url: String, data: Value) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.create_provider(data).await
}

#[tauri::command]
async fn get_provider(state: tauri::State<'_, AppState>, server_url: String, id: i32) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.get_provider(id).await
}

#[tauri::command]
async fn update_provider(state: tauri::State<'_, AppState>, server_url: String, id: i32, data: Value) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.update_provider(id, data).await
}

#[tauri::command]
async fn delete_provider(state: tauri::State<'_, AppState>, server_url: String, id: i32) -> Result<(), String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.delete_provider(id).await
}

#[tauri::command]
async fn fetch_models(state: tauri::State<'_, AppState>, server_url: String) -> Result<Vec<Value>, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.list_models().await
}

#[tauri::command]
async fn create_model(state: tauri::State<'_, AppState>, server_url: String, data: Value) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.create_model(data).await
}

#[tauri::command]
async fn get_model(state: tauri::State<'_, AppState>, server_url: String, id: i32) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.get_model(id).await
}

#[tauri::command]
async fn update_model(state: tauri::State<'_, AppState>, server_url: String, id: i32, data: Value) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.update_model(id, data).await
}

#[tauri::command]
async fn delete_model(state: tauri::State<'_, AppState>, server_url: String, id: i32) -> Result<(), String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.delete_model(id).await
}

#[tauri::command]
async fn fetch_api_keys(state: tauri::State<'_, AppState>, server_url: String) -> Result<Vec<Value>, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.list_api_keys().await
}

#[tauri::command]
async fn create_api_key(state: tauri::State<'_, AppState>, server_url: String, data: Value) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.create_api_key(data).await
}

#[tauri::command]
async fn get_api_key(state: tauri::State<'_, AppState>, server_url: String, id: i32) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.get_api_key(id).await
}

#[tauri::command]
async fn update_api_key(state: tauri::State<'_, AppState>, server_url: String, id: i32, data: Value) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.update_api_key(id, data).await
}

#[tauri::command]
async fn delete_api_key(state: tauri::State<'_, AppState>, server_url: String, id: i32) -> Result<(), String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.delete_api_key(id).await
}

#[tauri::command]
async fn fetch_provider_credentials(state: tauri::State<'_, AppState>, server_url: String, provider_id: Option<i32>) -> Result<Vec<Value>, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.list_provider_credentials(provider_id).await
}

#[tauri::command]
async fn create_provider_credential(state: tauri::State<'_, AppState>, server_url: String, data: Value) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.create_provider_credential(data).await
}

#[tauri::command]
async fn get_provider_credential(state: tauri::State<'_, AppState>, server_url: String, id: i32) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.get_provider_credential(id).await
}

#[tauri::command]
async fn update_provider_credential(state: tauri::State<'_, AppState>, server_url: String, id: i32, data: Value) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.update_provider_credential(id, data).await
}

#[tauri::command]
async fn delete_provider_credential(state: tauri::State<'_, AppState>, server_url: String, id: i32) -> Result<(), String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.delete_provider_credential(id).await
}

#[tauri::command]
async fn test_provider_credential(state: tauri::State<'_, AppState>, server_url: String, credential_id: i32, model_id: String, prompt: Option<String>) -> Result<Value, String> {
    let keypair = {
        let kp = state.keypair.lock().map_err(|e| e.to_string())?;
        kp.clone().ok_or_else(|| "Keypair not loaded. Please configure key paths in Settings.".to_string())?
    };
    let client = AdminClient::new(server_url, keypair);
    client.test_provider_credential(credential_id, model_id, prompt).await
}

#[tauri::command]
fn get_keypair_fingerprint(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    let keypair = state.keypair.lock().map_err(|e| e.to_string())?;
    Ok(keypair.as_ref().map(|k| k.fingerprint.clone()))
}

#[tauri::command]
fn load_keypair(state: tauri::State<'_, AppState>, priv_key_path: String, pub_key_path: String) -> Result<String, String> {
    let keypair = KeyPair::from_ssh_files(&priv_key_path, &pub_key_path)?;
    let fingerprint = keypair.fingerprint.clone();
    let mut state_keypair = state.keypair.lock().map_err(|e| e.to_string())?;
    *state_keypair = Some(keypair);
    Ok(fingerprint)
}

#[tauri::command]
async fn fetch_provider_models_by_url(base_url: String) -> Result<Vec<ProviderModel>, String> {
    fetch_provider_models(&base_url).await
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppState { keypair: Mutex::new(None) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_service_status,
            refresh_cache,
            fetch_providers,
            create_provider,
            get_provider,
            update_provider,
            delete_provider,
            fetch_models,
            create_model,
            get_model,
            update_model,
            delete_model,
            fetch_api_keys,
            create_api_key,
            get_api_key,
            update_api_key,
            delete_api_key,
            fetch_provider_credentials,
            create_provider_credential,
            get_provider_credential,
            update_provider_credential,
            delete_provider_credential,
            test_provider_credential,
            get_keypair_fingerprint,
            load_keypair,
            fetch_provider_models_by_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
