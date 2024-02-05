// authentication.rs

use crate::env_vars::jwt_secret;
use actix_web::dev::ServiceRequest;
use actix_web::{Error, HttpMessage, HttpResponse, ResponseError};
use actix_web_httpauth::extractors::bearer::{BearerAuth, Config};
use actix_web_httpauth::extractors::AuthenticationError;
use jsonwebtoken::{encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::time::Duration;

#[derive(Debug)]
struct WrappedJwtError(jsonwebtoken::errors::Error);

impl Display for WrappedJwtError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl ResponseError for WrappedJwtError {
    fn error_response(&self) -> HttpResponse {
        // Create an appropriate HTTP response for the wrapped JwtError
        // For example:
        HttpResponse::Unauthorized().finish()
    }
}

pub fn generate(user_id: i32) -> String {
    let created = std::time::SystemTime::now();
    let expires = created + Duration::new(31557600, 0);

    let claims = LoginClaims {
        user_id,
        expires,
        created,
    };

    let header = Header::default();
    encode(
        &header,
        &claims,
        &EncodingKey::from_secret(jwt_secret().as_ref()),
    )
    .unwrap()
}

pub fn decode(token: &str) -> Result<LoginClaims, Error> {
    jsonwebtoken::decode::<LoginClaims>(
        token,
        &DecodingKey::from_secret(jwt_secret().as_ref()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|e| Error::from(WrappedJwtError(e)))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginClaims {
    user_id: i32,
    expires: std::time::SystemTime,
    created: std::time::SystemTime,
}

pub async fn jwt_validator_generator(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    match decode(credentials.token()) {
        Ok(claims) => {
            req.extensions_mut().insert(claims);
            Ok(req)
        }
        Err(e) => Err((e, req)),
    }
}
