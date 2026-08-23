use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::db::StoreError;
use crate::response;
use crate::service::provider_credential_service;
use crate::util::encryption;
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateProviderCredentialRequest {
    pub provider_id: i32,
    pub name: Option<String>,
    pub api_key: String,
    pub account: Option<String>,
    pub password: Option<String>,
    pub priority: Option<i32>,
    pub is_active: Option<bool>,
}

fn deserialize_double_option<'de, D>(de: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(de)?))
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct UpdateProviderCredentialRequest {
    pub provider_id: Option<i32>,
    #[serde(deserialize_with = "deserialize_double_option")]
    pub name: Option<Option<String>>,
    pub api_key: Option<String>,
    #[serde(deserialize_with = "deserialize_double_option")]
    pub account: Option<Option<String>>,
    pub password: Option<String>,
    pub priority: Option<i32>,
    pub is_active: Option<bool>,
}

#[derive(Deserialize)]
pub struct ListCredentialsQuery {
    pub provider_id: Option<i32>,
}

#[derive(Serialize)]
pub struct ProviderCredentialResponse {
    pub id: i32,
    pub provider_id: i32,
    pub name: String,
    pub api_key: String,
    pub account: Option<String>,
    pub password: Option<String>,
    pub priority: i32,
    pub is_active: bool,
    pub quota_exhausted: bool,
    pub created_time: chrono::NaiveDateTime,
    pub last_updated: chrono::NaiveDateTime,
}

impl ProviderCredentialResponse {
    fn from_model(m: crate::db::entities::provider_credential::Model, key: &[u8; 32]) -> Result<Self, String> {
        let api_key = encryption::decrypt(&m.api_key, key)?;
        let password = m.encrypted_password.map(|p| encryption::decrypt(&p, key)).transpose()?;
        let masked_key = if api_key.len() > 4 {
            format!("{}****", &api_key[..4])
        } else {
            "****".to_string()
        };
        Ok(ProviderCredentialResponse {
            id: m.id,
            provider_id: m.provider_id,
            name: m.name,
            api_key: masked_key,
            account: m.account,
            password: password.map(|_| "****".to_string()),
            priority: m.priority,
            is_active: m.is_active,
            quota_exhausted: m.quota_exhausted,
            created_time: m.created_time,
            last_updated: m.last_updated,
        })
    }
}

fn credential_result_response(
    result: Result<crate::db::entities::provider_credential::Model, StoreError>,
    encryption_key: &[u8; 32],
) -> HttpResponse {
    match result {
        Ok(m) => match ProviderCredentialResponse::from_model(m, encryption_key) {
            Ok(resp) => HttpResponse::Ok().json(resp),
            Err(e) => {
                log::error!("Decrypt error: {}", e);
                response::from_store_error(StoreError::Database(e))
            }
        },
        Err(e) => {
            log::error!("{}", e);
            response::from_store_error(e)
        }
    }
}

pub async fn list_provider_credentials(
    state: web::Data<AppState>,
    query: web::Query<ListCredentialsQuery>,
) -> HttpResponse {
    let credentials = match query.provider_id {
        Some(pid) => state.database.provider_credentials.list_by_provider(pid).await,
        None => state.database.provider_credentials.list().await,
    };
    match credentials {
        Ok(list) => {
            let results: Vec<ProviderCredentialResponse> = list
                .into_iter()
                .filter_map(|m| ProviderCredentialResponse::from_model(m, &state.encryption_key).ok())
                .collect();
            HttpResponse::Ok().json(results)
        }
        Err(e) => {
            log::error!("{}", e);
            response::db_error()
        }
    }
}

pub async fn get_provider_credential(
    state: web::Data<AppState>,
    path: web::Path<i32>,
) -> HttpResponse {
    match state.database.provider_credentials.get(path.into_inner()).await {
        Ok(Some(m)) => credential_result_response(Ok(m), &state.encryption_key),
        Ok(None) => response::not_found("Credential not found"),
        Err(e) => credential_result_response(Err(e), &state.encryption_key),
    }
}

pub async fn create_provider_credential(
    state: web::Data<AppState>,
    body: web::Json<CreateProviderCredentialRequest>,
) -> HttpResponse {
    if body.api_key.trim().is_empty() {
        return response::bad_request("API key must not be empty");
    }
    let input = provider_credential_service::CredentialInput {
        provider_id: body.provider_id,
        name: body.name.clone().unwrap_or_default(),
        api_key: body.api_key.clone(),
        account: body.account.clone(),
        password: body.password.clone(),
        priority: body.priority.unwrap_or(0),
        is_active: body.is_active.unwrap_or(true),
    };
    let result = provider_credential_service::create(
        &state.database.provider_credentials,
        input,
        &state.encryption_key,
    ).await;
    credential_result_response(result, &state.encryption_key)
}

pub async fn update_provider_credential(
    state: web::Data<AppState>,
    path: web::Path<i32>,
    body: web::Json<UpdateProviderCredentialRequest>,
) -> HttpResponse {
    let update = provider_credential_service::CredentialUpdate {
        provider_id: body.provider_id,
        name: body.name.clone(),
        new_api_key: body.api_key.clone(),
        account: body.account.clone(),
        password: body.password.clone(),
        priority: body.priority,
        is_active: body.is_active,
    };
    let result = provider_credential_service::update(
        &state.database.provider_credentials,
        path.into_inner(),
        update,
        &state.encryption_key,
    ).await;
    credential_result_response(result, &state.encryption_key)
}

pub async fn delete_provider_credential(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_delete!(state.database.provider_credentials.delete(path.into_inner()).await, "Credential not found")
}

pub async fn reset_provider_credential_status(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    let id = path.into_inner();
    match state.database.provider_credentials.clear_quota_exhausted(id).await {
        Ok(()) => {
            state.priority_penalty.reset_credential(id);
            HttpResponse::Ok().json(serde_json::json!({"success": true}))
        }
        Err(e) => {
            log::error!("{}", e);
            response::from_store_error(e)
        }
    }
}
