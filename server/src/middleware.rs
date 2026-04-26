//! Middleware bearer-token: valida `Authorization: Bearer <token>`
//! contra `common::auth_token()`. Sin header válido: 401.

use std::future::{ready, Ready};

use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpResponse,
};
use futures_util::future::LocalBoxFuture;

use common::{auth_token, AUTH_HEADER};

pub struct TokenAuth;

impl<S, B> Transform<S, ServiceRequest> for TokenAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = TokenAuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(TokenAuthMiddleware { service }))
    }
}

pub struct TokenAuthMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for TokenAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let header = req
            .headers()
            .get(AUTH_HEADER)
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        let expected = format!("Bearer {}", auth_token());
        let ok = header.as_deref() == Some(expected.as_str());

        if !ok {
            let (req, _pl) = req.into_parts();
            let res = HttpResponse::Unauthorized()
                .json(serde_json::json!({ "error": "missing or invalid bearer token" }));
            return Box::pin(async move {
                Ok(ServiceResponse::new(req, res).map_into_right_body())
            });
        }

        let fut = self.service.call(req);
        Box::pin(async move {
            let res = fut.await?;
            Ok(res.map_into_left_body())
        })
    }
}
