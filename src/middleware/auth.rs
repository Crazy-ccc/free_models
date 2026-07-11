use std::future::{ready, Ready};
use std::rc::Rc;

use actix_web::body::{BoxBody, MessageBody};
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{web, Error, HttpResponse};
use futures_util::future::LocalBoxFuture;

use crate::error;
use crate::service::api_key_cache::ApiKeyCache;

pub struct AuthMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Transform = AuthMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddlewareService {
            service: Rc::new(service),
        }))
    }
}

#[derive(Clone)]
pub struct AuthMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let key = match extract_bearer(&req) {
            Ok(k) => k,
            Err(resp) => {
                return Box::pin(async move {
                    Ok(req.into_response(resp).map_into_boxed_body())
                });
            }
        };

        let api_key_cache = match req.app_data::<web::Data<ApiKeyCache>>() {
            Some(cache) => cache.get_ref().clone(),
            None => {
                return Box::pin(async move {
                    Ok(req
                        .into_response(error::internal_error("ApiKey cache not available"))
                        .map_into_boxed_body())
                });
            }
        };

        let (http_req, payload) = req.into_parts();
        let service = self.service.clone();

        Box::pin(async move {
            let req = ServiceRequest::from_parts(http_req, payload);

            if api_key_cache.contains(&key) {
                let res = service.call(req).await?;
                Ok(res.map_into_boxed_body())
            } else {
                Ok(req
                    .into_response(error::unauthorized())
                    .map_into_boxed_body())
            }
        })
    }
}

/// 从请求头提取 `Bearer <key>` 中的 key。
/// 缺失 `Authorization` 头或格式错误时返回 `error::unauthorized()` 响应。
fn extract_bearer(req: &ServiceRequest) -> Result<String, HttpResponse> {
    let header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| error::unauthorized())?;

    let key = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| error::unauthorized())?;

    Ok(key.to_string())
}
