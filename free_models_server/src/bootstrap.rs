use std::env;

use log::{debug, info, warn};

use crate::db::impls::AdminKeyStoreSeaorm;
use crate::middleware::admin_auth::{compute_fingerprint, parse_openssh_ed25519_pubkey};

pub async fn ensure_bootstrap_admin_key(admin_keys: &AdminKeyStoreSeaorm) {
    let raw = match env::var("ADMIN_BOOTSTRAP_PUBLIC_KEY") {
        Ok(v) => v.trim().to_string(),
        Err(_) => String::new(),
    };
    if raw.is_empty() {
        let count = admin_keys.count_all().await.expect("count admin_key failed");
        if count == 0 {
            warn!("No admin public key configured: Admin API is unusable until one exists. Set ADMIN_BOOTSTRAP_PUBLIC_KEY (ssh-keygen -t ed25519) and restart.");
        }
        return;
    }
    let pubkey_bytes = match parse_openssh_ed25519_pubkey(&raw) {
        Some(bytes) => bytes,
        None => panic!(
            "ADMIN_BOOTSTRAP_PUBLIC_KEY is not a valid OpenSSH Ed25519 public key. Generate one with: ssh-keygen -t ed25519"
        ),
    };
    let fingerprint = compute_fingerprint(&pubkey_bytes);
    if let Some(existing) = admin_keys
        .find_by_fingerprint(&fingerprint)
        .await
        .expect("query admin_key failed")
    {
        if existing.is_active {
            debug!(
                "Bootstrap admin key already exists (fingerprint {}), skip",
                fingerprint
            );
        } else {
            debug!(
                "Bootstrap admin key exists but is deactivated (fingerprint {}), skip and keep deactivated",
                fingerprint
            );
        }
        return;
    }
    let name = env::var("ADMIN_BOOTSTRAP_KEY_NAME").unwrap_or_else(|_| "bootstrap".to_string());
    let name = if name.trim().is_empty() {
        "bootstrap".to_string()
    } else {
        name
    };
    admin_keys
        .create(&name, &raw, &fingerprint)
        .await
        .expect("insert bootstrap admin key failed");
    info!(
        "Bootstrap admin key inserted (name: {}, fingerprint: {})",
        name, fingerprint
    );
}

#[cfg(test)]
mod bootstrap_tests {
    use std::sync::Mutex;

    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvCleanup;

    impl Drop for EnvCleanup {
        fn drop(&mut self) {
            unsafe { env::remove_var("ADMIN_BOOTSTRAP_PUBLIC_KEY") };
            unsafe { env::remove_var("ADMIN_BOOTSTRAP_KEY_NAME") };
        }
    }

    fn make_ssh_pubkey(key_bytes: [u8; 32]) -> String {
        use base64::Engine;
        let mut inner = Vec::new();
        inner.extend_from_slice(&11u32.to_be_bytes());
        inner.extend_from_slice(b"ssh-ed25519");
        inner.extend_from_slice(&32u32.to_be_bytes());
        inner.extend_from_slice(&key_bytes);
        format!(
            "ssh-ed25519 {} test@host",
            base64::engine::general_purpose::STANDARD.encode(inner)
        )
    }

    fn raw_row(
        name: &str,
        key_bytes: [u8; 32],
        is_active: bool,
    ) -> crate::db::entities::admin_key::ActiveModel {
        crate::db::entities::admin_key::ActiveModel {
            name: Set(name.to_string()),
            public_key: Set(make_ssh_pubkey(key_bytes)),
            fingerprint: Set(Some(compute_fingerprint(&key_bytes))),
            is_active: Set(is_active),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn first_bootstrap_inserts_active_key() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _cleanup = EnvCleanup;
        let pubkey = make_ssh_pubkey([9u8; 32]);
        unsafe { env::set_var("ADMIN_BOOTSTRAP_PUBLIC_KEY", pubkey.clone()) };

        let db = crate::db::test_support::connect_in_memory_db().await;
        let store = AdminKeyStoreSeaorm::new(db.clone());
        ensure_bootstrap_admin_key(&store).await;
        assert_eq!(store.count_all().await.unwrap(), 1);

        let row = crate::db::entities::admin_key::Entity::find()
            .one(&db)
            .await
            .unwrap()
            .expect("row should exist");
        assert_eq!(
            row.fingerprint.as_deref(),
            Some(compute_fingerprint(&[9u8; 32]).as_str())
        );
        assert!(row.is_active);
        assert_eq!(row.name, "bootstrap");
        assert_eq!(row.public_key, pubkey);

    }

    #[tokio::test]
    async fn bootstrap_with_custom_name() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _cleanup = EnvCleanup;
        unsafe { env::set_var("ADMIN_BOOTSTRAP_PUBLIC_KEY", make_ssh_pubkey([10u8; 32])) };
        unsafe { env::set_var("ADMIN_BOOTSTRAP_KEY_NAME", "ops-key") };

        let db = crate::db::test_support::connect_in_memory_db().await;
        let store = AdminKeyStoreSeaorm::new(db.clone());
        ensure_bootstrap_admin_key(&store).await;
        assert_eq!(store.count_all().await.unwrap(), 1);

        let row = crate::db::entities::admin_key::Entity::find()
            .one(&db)
            .await
            .unwrap()
            .expect("row should exist");
        assert_eq!(row.name, "ops-key");
        assert!(row.is_active);

    }

    #[tokio::test]
    async fn idempotent_on_restart() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _cleanup = EnvCleanup;
        unsafe { env::set_var("ADMIN_BOOTSTRAP_PUBLIC_KEY", make_ssh_pubkey([11u8; 32])) };

        let db = crate::db::test_support::connect_in_memory_db().await;
        let store = AdminKeyStoreSeaorm::new(db);
        ensure_bootstrap_admin_key(&store).await;
        ensure_bootstrap_admin_key(&store).await;
        assert_eq!(store.count_all().await.unwrap(), 1);

    }

    #[tokio::test]
    async fn deactivated_key_not_revived() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _cleanup = EnvCleanup;
        let fingerprint = compute_fingerprint(&[12u8; 32]);
        unsafe { env::set_var("ADMIN_BOOTSTRAP_PUBLIC_KEY", make_ssh_pubkey([12u8; 32])) };

        let db = crate::db::test_support::connect_in_memory_db().await;
        raw_row("dead-key", [12u8; 32], false)
            .insert(&db)
            .await
            .expect("insert deactivated row");

        let store = AdminKeyStoreSeaorm::new(db.clone());
        ensure_bootstrap_admin_key(&store).await;
        assert_eq!(store.count_all().await.unwrap(), 1);

        let row = store
            .find_by_fingerprint(&fingerprint)
            .await
            .unwrap()
            .expect("row should exist");
        assert!(!row.is_active);

    }

    #[tokio::test]
    async fn additional_key_appended() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _cleanup = EnvCleanup;
        unsafe { env::set_var("ADMIN_BOOTSTRAP_PUBLIC_KEY", make_ssh_pubkey([14u8; 32])) };

        let db = crate::db::test_support::connect_in_memory_db().await;
        raw_row("existing", [13u8; 32], true)
            .insert(&db)
            .await
            .expect("insert existing row");

        let store = AdminKeyStoreSeaorm::new(db.clone());
        assert_eq!(store.count_all().await.unwrap(), 1);
        ensure_bootstrap_admin_key(&store).await;
        assert_eq!(store.count_all().await.unwrap(), 2);

        let new_row = store
            .find_by_fingerprint(&compute_fingerprint(&[14u8; 32]))
            .await
            .unwrap()
            .expect("bootstrapped row should exist");
        assert!(new_row.is_active);

    }

    #[tokio::test]
    #[should_panic(expected = "ADMIN_BOOTSTRAP_PUBLIC_KEY")]
    async fn invalid_key_panics() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _cleanup = EnvCleanup;
        unsafe { env::set_var("ADMIN_BOOTSTRAP_PUBLIC_KEY", "garbage-not-a-key") };

        let db = crate::db::test_support::connect_in_memory_db().await;
        let store = AdminKeyStoreSeaorm::new(db);
        ensure_bootstrap_admin_key(&store).await;

    }

    #[tokio::test]
    async fn missing_env_empty_table_noop() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _cleanup = EnvCleanup;

        let db = crate::db::test_support::connect_in_memory_db().await;
        let store = AdminKeyStoreSeaorm::new(db);
        ensure_bootstrap_admin_key(&store).await;
        assert_eq!(store.count_all().await.unwrap(), 0);
    }
}
