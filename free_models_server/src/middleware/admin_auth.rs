use std::future::{ready, Ready};
use std::rc::Rc;

use actix_web::body::{BoxBody, MessageBody};
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{web, Error, HttpResponse};
use base64::Engine;
use ed25519_dalek::Verifier;
use futures_util::future::LocalBoxFuture;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::db::entities::admin_key;
use crate::AppState;

pub struct AdminAuthMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AdminAuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Transform = AdminAuthMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AdminAuthMiddlewareService {
            service: Rc::new(service),
        }))
    }
}

#[derive(Clone)]
pub struct AdminAuthMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for AdminAuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let fingerprint = match req
            .headers()
            .get("X-Admin-Fingerprint")
            .and_then(|v| v.to_str().ok())
        {
            Some(v) => v.to_string(),
            None => {
                return Box::pin(async move {
                    Ok(req
                        .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                            "error": "Missing X-Admin-Fingerprint header"
                        })))
                        .map_into_boxed_body())
                });
            }
        };

        let timestamp_str = match req
            .headers()
            .get("X-Admin-Timestamp")
            .and_then(|v| v.to_str().ok())
        {
            Some(v) => v.to_string(),
            None => {
                return Box::pin(async move {
                    Ok(req
                        .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                            "error": "Missing X-Admin-Timestamp header"
                        })))
                        .map_into_boxed_body())
                });
            }
        };

        let signature = match req
            .headers()
            .get("X-Admin-Signature")
            .and_then(|v| v.to_str().ok())
        {
            Some(v) => v.to_string(),
            None => {
                return Box::pin(async move {
                    Ok(req
                        .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                            "error": "Missing X-Admin-Signature header"
                        })))
                        .map_into_boxed_body())
                });
            }
        };

        let timestamp: u64 = match timestamp_str.parse() {
            Ok(t) => t,
            Err(_) => {
                return Box::pin(async move {
                    Ok(req
                        .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                            "error": "Invalid timestamp"
                        })))
                        .map_into_boxed_body())
                });
            }
        };

        let now = chrono::Utc::now().timestamp() as u64;
        if now.saturating_sub(timestamp) > 300 {
            return Box::pin(async move {
                Ok(req
                    .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                        "error": "Timestamp expired"
                    })))
                    .map_into_boxed_body())
            });
        }

        let db = match req.app_data::<web::Data<AppState>>() {
            Some(state) => state.db.clone(),
            None => {
                return Box::pin(async move {
                    Ok(req
                        .into_response(HttpResponse::InternalServerError().json(
                            serde_json::json!({
                                "error": "Database not available"
                            }),
                        ))
                        .map_into_boxed_body())
                });
            }
        };

        let method = req.method().to_string();
        let path = req.path().to_string();
        let service = self.service.clone();

        Box::pin(async move {
            let admin_record = match admin_key::Entity::find()
                .filter(admin_key::Column::Fingerprint.eq(&fingerprint))
                .filter(admin_key::Column::IsActive.eq(true))
                .one(&db)
                .await
            {
                Ok(Some(record)) => record,
                Ok(None) => {
                    return Ok(req
                        .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                            "error": "Invalid fingerprint"
                        })))
                        .map_into_boxed_body());
                }
                Err(e) => {
                    eprintln!("Database error in admin auth: {}", e);
                    return Ok(req
                        .into_response(HttpResponse::InternalServerError().json(
                            serde_json::json!({
                                "error": "Database error"
                            }),
                        ))
                        .map_into_boxed_body());
                }
            };

            let raw_pubkey = match parse_openssh_ed25519_pubkey(&admin_record.public_key) {
                Some(key) => key,
                None => {
                    return Ok(req
                        .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                            "error": "Invalid public key format"
                        })))
                        .map_into_boxed_body());
                }
            };

            let payload = format!("{}:{}:{}", method, path, timestamp);
            let signature_bytes = match base64::engine::general_purpose::STANDARD.decode(&signature)
            {
                Ok(b) => b,
                Err(_) => {
                    return Ok(req
                        .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                            "error": "Invalid signature encoding"
                        })))
                        .map_into_boxed_body());
                }
            };

            if signature_bytes.len() != 64 {
                return Ok(req
                    .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                        "error": "Invalid signature length"
                    })))
                    .map_into_boxed_body());
            }

            let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&raw_pubkey) {
                Ok(k) => k,
                Err(_) => {
                    return Ok(req
                        .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                            "error": "Invalid public key"
                        })))
                        .map_into_boxed_body());
                }
            };

            let signature_array: [u8; 64] = match signature_bytes.try_into() {
                Ok(arr) => arr,
                Err(_) => {
                    return Ok(req
                        .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                            "error": "Invalid signature length"
                        })))
                        .map_into_boxed_body());
                }
            };

            let sig = ed25519_dalek::Signature::from_bytes(&signature_array);

            if verifying_key.verify(payload.as_bytes(), &sig).is_err() {
                return Ok(req
                    .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                        "error": "Signature verification failed"
                    })))
                    .map_into_boxed_body());
            }

            let res = service.call(req).await?;
            Ok(res.map_into_boxed_body())
        })
    }
}

pub fn parse_openssh_ed25519_pubkey(pubkey_str: &str) -> Option<[u8; 32]> {
    let parts: Vec<&str> = pubkey_str.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let encoded = parts[1];
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    if decoded.len() < 51 {
        return None;
    }
    let type_len =
        u32::from_be_bytes([decoded[0], decoded[1], decoded[2], decoded[3]]) as usize;
    if type_len != 11 {
        return None;
    }
    if &decoded[4..15] != b"ssh-ed25519" {
        return None;
    }
    let key_len =
        u32::from_be_bytes([decoded[15], decoded[16], decoded[17], decoded[18]]) as usize;
    if key_len != 32 {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded[19..51]);
    Some(key)
}

pub fn compute_fingerprint(raw_pubkey: &[u8; 32]) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(raw_pubkey);
    format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(&hash)
    )
}
