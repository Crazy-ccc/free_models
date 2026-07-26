use std::cell::RefCell;
use std::collections::HashMap;
use std::future::{ready, Ready};
use std::rc::Rc;

use actix_web::body::{BoxBody, MessageBody};
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{web, Error, HttpResponse};
use base64::Engine;
use ed25519_dalek::Verifier;
use futures_util::future::LocalBoxFuture;
use crate::AppState;

fn auth_response(
    req: ServiceRequest,
    msg: impl Into<String>,
    builder: fn() -> actix_web::HttpResponseBuilder,
) -> ServiceResponse<BoxBody> {
    let msg = msg.into();
    req.into_response(builder().json(serde_json::json!({"error": msg})))
        .map_into_boxed_body()
}

fn require_header(req: &ServiceRequest, name: &str) -> Result<String, ()> {
    req.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
        .ok_or(())
}

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
            used_nonces: Rc::new(RefCell::new(HashMap::new())),
        }))
    }
}

#[derive(Clone)]
pub struct AdminAuthMiddlewareService<S> {
    service: Rc<S>,
    used_nonces: Rc<RefCell<HashMap<String, u64>>>,
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
        let fingerprint = match require_header(&req, "X-Admin-Fingerprint") {
            Ok(v) => v,
            Err(()) => return Box::pin(async move { Ok(auth_response(req, "Missing X-Admin-Fingerprint header", HttpResponse::Unauthorized)) }),
        };

        let timestamp_str = match require_header(&req, "X-Admin-Timestamp") {
            Ok(v) => v,
            Err(()) => return Box::pin(async move { Ok(auth_response(req, "Missing X-Admin-Timestamp header", HttpResponse::Unauthorized)) }),
        };

        let signature = match require_header(&req, "X-Admin-Signature") {
            Ok(v) => v,
            Err(()) => return Box::pin(async move { Ok(auth_response(req, "Missing X-Admin-Signature header", HttpResponse::Unauthorized)) }),
        };

        let nonce = req
            .headers()
            .get("X-Admin-Nonce")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_string());

        let body_hash = req
            .headers()
            .get("X-Admin-Body-Hash")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_string());

        let timestamp: u64 = match timestamp_str.parse() {
            Ok(t) => t,
            Err(_) => return Box::pin(async move { Ok(auth_response(req, "Invalid timestamp", HttpResponse::Unauthorized)) }),
        };

        let now = chrono::Utc::now().timestamp() as u64;
        if now.saturating_sub(timestamp) > 300 {
            return Box::pin(async move { Ok(auth_response(req, "Timestamp expired", HttpResponse::Unauthorized)) });
        }

        if let Some(ref n) = nonce {
            let mut cache = self.used_nonces.borrow_mut();
            cache.retain(|_, ts| now.saturating_sub(*ts) <= 300);
            if cache.contains_key(n) {
                return Box::pin(async move { Ok(auth_response(req, "Nonce already used", HttpResponse::Unauthorized)) });
            }
            cache.insert(n.clone(), now);
        }

        let admin_keys = match req.app_data::<web::Data<AppState>>() {
            Some(state) => state.database.admin_keys.clone(),
            None => return Box::pin(async move { Ok(auth_response(req, "Database not available", HttpResponse::InternalServerError)) }),
        };

        let method = req.method().to_string();
        let path = req.path().to_string();
        let nonce_part = nonce.as_deref().unwrap_or("").to_string();
        let body_hash_part = body_hash.as_deref().unwrap_or("").to_string();
        let service = self.service.clone();

        Box::pin(async move {
            let admin_record = match admin_keys.find_active_by_fingerprint(&fingerprint).await {
                Ok(Some(record)) => record,
                Ok(None) => return Ok(auth_response(req, "Invalid fingerprint", HttpResponse::Unauthorized)),
                Err(e) => {
                    log::error!("Database error in admin auth: {}", e);
                    return Ok(auth_response(req, "Database error", HttpResponse::InternalServerError));
                }
            };

            let raw_pubkey = match parse_openssh_ed25519_pubkey(&admin_record.public_key) {
                Some(key) => key,
                None => return Ok(auth_response(req, "Invalid public key format", HttpResponse::Unauthorized)),
            };

            let payload = format!("{}:{}:{}:{}:{}", method, path, timestamp, nonce_part, body_hash_part);
            let signature_bytes = match base64::engine::general_purpose::STANDARD.decode(&signature) {
                Ok(b) => b,
                Err(_) => return Ok(auth_response(req, "Invalid signature encoding", HttpResponse::Unauthorized)),
            };

            if signature_bytes.len() != 64 {
                return Ok(auth_response(req, "Invalid signature length", HttpResponse::Unauthorized));
            }

            let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&raw_pubkey) {
                Ok(k) => k,
                Err(_) => return Ok(auth_response(req, "Invalid public key", HttpResponse::Unauthorized)),
            };

            let signature_array: [u8; 64] = match signature_bytes.try_into() {
                Ok(arr) => arr,
                Err(_) => return Ok(auth_response(req, "Invalid signature length", HttpResponse::Unauthorized)),
            };

            let sig = ed25519_dalek::Signature::from_bytes(&signature_array);

            if verifying_key.verify(payload.as_bytes(), &sig).is_err() {
                return Ok(auth_response(req, "Signature verification failed", HttpResponse::Unauthorized));
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
