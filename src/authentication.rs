use std::time::{Duration, SystemTime, SystemTimeError, UNIX_EPOCH};

use actix_web::dev::ServiceRequest;
use actix_web::http::StatusCode;
use actix_web::{Error, HttpMessage, ResponseError};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use diesel::{QueryDsl as _, RunQueryDsl as _};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::env_vars::jwt_secret;
use crate::model::User;
use crate::AppState;

#[derive(thiserror::Error, Debug)]
enum AuthenticationError {
    #[error("Token decode error: {0}")]
    TokenDecodeError(jsonwebtoken::errors::Error),
    #[error("User not found")]
    UserNotFound(#[from] diesel::result::Error),
    #[error("Token expired")]
    TokenExpired,
}

impl ResponseError for AuthenticationError {
    fn status_code(&self) -> StatusCode { StatusCode::UNAUTHORIZED }
}

pub fn generate(user_id: i32) -> String {
    let created = SystemTime::now();
    let expires = created + Duration::new(31557600, 0);

    let claims =
        Rfc7519Claims::try_from(LoginClaims { user_id, expires_at: expires, created_at: created })
            .expect("System time before unix epoch");

    let header = Header::default();
    jsonwebtoken::encode(&header, &claims, &EncodingKey::from_secret(jwt_secret().as_ref()))
        .expect("Encoding JWT token failed")
}

pub fn decode(token: &str) -> Result<Rfc7519Claims, jsonwebtoken::errors::Error> {
    jsonwebtoken::decode::<Rfc7519Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret().as_ref()),
        &Validation::default(),
    )
    .map(|data| data.claims)
}

#[derive(Debug)]
pub struct LoginClaims {
    user_id: i32,
    expires_at: SystemTime,
    created_at: SystemTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Rfc7519Claims {
    sub: i32,
    exp: u64,
    iat: u64,
}

impl From<Rfc7519Claims> for LoginClaims {
    fn from(value: Rfc7519Claims) -> Self {
        Self {
            user_id: value.sub,
            expires_at: SystemTime::from(UNIX_EPOCH) + Duration::new(value.exp, 0),
            created_at: SystemTime::from(UNIX_EPOCH) + Duration::new(value.iat, 0),
        }
    }
}

impl TryFrom<LoginClaims> for Rfc7519Claims {
    type Error = SystemTimeError;

    fn try_from(value: LoginClaims) -> Result<Self, Self::Error> {
        Ok(Self {
            sub: value.user_id,
            exp: value.expires_at.duration_since(UNIX_EPOCH)?.as_secs(),
            iat: value.created_at.duration_since(UNIX_EPOCH)?.as_secs(),
        })
    }
}

pub async fn jwt_validator_generator(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    use crate::schema::users::dsl::*;
    match decode(credentials.token()) {
        Ok(claims) => {
            let login_claims = LoginClaims::from(claims);
            if login_claims.expires_at < SystemTime::now() {
                return Err((Error::from(AuthenticationError::TokenExpired), req));
            }

            let find_result: Result<User, _> = users.find(login_claims.user_id).first(
                &mut req
                    .app_data::<actix_web::web::Data<AppState>>()
                    .expect("AppState should be defined")
                    .cpool(),
            );
            match find_result {
                Ok(user) => {
                    req.extensions_mut().insert(user);
                    Ok(req)
                }
                Err(e) => Err((Error::from(AuthenticationError::UserNotFound(e)), req)),
            }
        }
        Err(e) => Err((Error::from(AuthenticationError::TokenDecodeError(e)), req)),
    }
}
