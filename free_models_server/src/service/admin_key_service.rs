use log::{error, info};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set, ActiveModelTrait};
use crate::db::entities::admin_key;
use crate::middleware::admin_auth::{compute_fingerprint, parse_openssh_ed25519_pubkey};

pub async fn auto_fill_fingerprints(db: &DatabaseConnection) {
    let records = match admin_key::Entity::find()
        .filter(admin_key::Column::Fingerprint.is_null())
        .all(db)
        .await
    {
        Ok(records) => records,
        Err(e) => {
            error!("Failed to query admin_key table: {}", e);
            return;
        }
    };

    if records.is_empty() {
        return;
    }

    let mut updated_count = 0;
    for record in records {
        let raw_pubkey = match parse_openssh_ed25519_pubkey(&record.public_key) {
            Some(key) => key,
            None => {
                error!(
                    "Failed to parse public key for admin_key id={}: {}",
                    record.id, record.public_key
                );
                continue;
            }
        };

        let fingerprint = compute_fingerprint(&raw_pubkey);

        let mut active_model: admin_key::ActiveModel = record.into();
        active_model.fingerprint = Set(Some(fingerprint));

        match active_model.update(db).await {
            Ok(_) => {
                updated_count += 1;
            }
            Err(e) => {
                error!("Failed to update admin_key: {}", e);
            }
        }
    }

    if updated_count > 0 {
        info!("Auto-filled {} fingerprints in admin_key table", updated_count);
    }
}