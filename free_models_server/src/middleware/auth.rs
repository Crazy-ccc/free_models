use std::future::{ready, Ready};
use std::rc::Rc;

use actix_web::body::{BoxBody, MessageBody};
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{web, Error, HttpResponse};
use futures_util::future::LocalBoxFuture;

use crate::response;
use crate::AppState;

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
        // /health 和 /admin/* 端点跳过鉴权
        if req.path() == "/health" || req.path().starts_with("/admin/") {
            let service = self.service.clone();
            return Box::pin(async move {
                let res = service.call(req).await?;
                Ok(res.map_into_boxed_body())
            });
        }

        let key = match extract_bearer(&req) {
            Ok(k) => k,
            Err(resp) => {
                return Box::pin(async move {
                    Ok(req.into_response(resp).map_into_boxed_body())
                });
            }
        };

        let api_key_cache = match req.app_data::<web::Data<AppState>>() {
            Some(state) => state.api_key_cache.clone(),
            None => {
                let err = internal_error_for(&req, "AppState not available");
                return Box::pin(async move {
                    Ok(req
                        .into_response(err)
                        .map_into_boxed_body())
                });
            }
        };

        let (http_req, payload) = req.into_parts();
        let service = self.service.clone();

        Box::pin(async move {
            let req = ServiceRequest::from_parts(http_req, payload);

            if api_key_cache.contains(&key).await {
                let res = service.call(req).await?;
                Ok(res.map_into_boxed_body())
            } else {
                let err = unauthorized_for(&req);
                Ok(req
                    .into_response(err)
                    .map_into_boxed_body())
            }
        })
    }
}

/// 根据请求路径返回对应协议格式的 401 未授权响应
fn unauthorized_for(req: &ServiceRequest) -> HttpResponse {
    if is_anthropic_request(req) {
        response::anthropic_unauthorized()
    } else {
        response::unauthorized()
    }
}

/// 根据请求路径返回对应协议格式的 500 内部错误响应
fn internal_error_for(req: &ServiceRequest, message: &str) -> HttpResponse {
    if is_anthropic_request(req) {
        response::anthropic_internal_error(message)
    } else {
        response::internal_error(message)
    }
}

/// 是否为 Anthropic 协议请求（/v1/messages）
fn is_anthropic_request(req: &ServiceRequest) -> bool {
    req.path().starts_with("/v1/messages")
}

/// 从请求头提取 `Bearer <key>` 中的 key。
/// 缺失 `Authorization` 头或格式错误时根据请求路径返回对应格式的错误响应。
fn extract_bearer(req: &ServiceRequest) -> Result<String, HttpResponse> {
    let header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| unauthorized_for(req))?;

    let key = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| unauthorized_for(req))?;

    Ok(key.to_string())
}
