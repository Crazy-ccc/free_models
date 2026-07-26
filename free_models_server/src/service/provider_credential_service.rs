use crate::db::entities::provider_credential::Model as ProviderCredential;
use crate::db::impls::ProviderCredentialStoreSeaorm;
use crate::db::StoreError;

use crate::util::encryption;

// 重导出聚合结构体，便于调用方通过 service 模块路径访问（向后兼容）。
pub use crate::db::impls::provider_credential::{CredentialInput, CredentialUpdate};

pub async fn create(
    store: &ProviderCredentialStoreSeaorm,
    mut input: CredentialInput,
    encryption_key: &[u8; 32],
) -> Result<ProviderCredential, StoreError> {
    input.api_key = encryption::encrypt(&input.api_key, encryption_key)
        .map_err(|e| StoreError::Database(e.to_string()))?;
    let encrypted_password = input.password
        .as_ref()
        .map(|p| encryption::encrypt(p, encryption_key).map_err(|e| StoreError::Database(e.to_string())))
        .transpose()?;
    input.password = encrypted_password;
    store.create(&input).await
}

pub async fn update(
    store: &ProviderCredentialStoreSeaorm,
    id: i32,
    mut update: CredentialUpdate,
    encryption_key: &[u8; 32],
) -> Result<ProviderCredential, StoreError> {
    let encrypted_new_api_key = update.new_api_key
        .as_ref()
        .map(|key| encryption::encrypt(key, encryption_key).map_err(|e| StoreError::Database(e.to_string())))
        .transpose()?;
    update.new_api_key = encrypted_new_api_key;
    let encrypted_password = update.password
        .as_ref()
        .map(|p| encryption::encrypt(p, encryption_key).map_err(|e| StoreError::Database(e.to_string())))
        .transpose()?;
    update.password = encrypted_password;
    store.update(id, &update).await
}
