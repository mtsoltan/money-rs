use std::fmt::Debug;
use std::sync::Arc;
use std::{fmt, ops};

use actix_http::{Payload, StatusCode};
use actix_web::{Error, FromRequest, HttpRequest, HttpResponse, ResponseError};
use log::error;
use serde::de::DeserializeOwned;

#[derive(thiserror::Error, Debug)]
pub enum ArrayQueryPayloadError {
    /// Query deserialize error.
    #[error("Query deserialize error: {0}")]
    Deserialize(#[from] serde_qs::Error),
}

impl ResponseError for ArrayQueryPayloadError {
    fn status_code(&self) -> StatusCode { StatusCode::BAD_REQUEST }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArrayQuery<T>(pub T);

impl<T> ArrayQuery<T> {
    /// Unwrap into inner `T` value.
    #[allow(dead_code)] // Used internally by actix
    pub fn into_inner(self) -> T { self.0 }
}

impl<T: DeserializeOwned> ArrayQuery<T> {
    #[allow(dead_code)] // Used internally by actix
    pub fn from_query(query_str: &str) -> Result<Self, ArrayQueryPayloadError> {
        serde_qs::from_str::<T>(query_str).map(Self).map_err(ArrayQueryPayloadError::Deserialize)
    }
}

impl<T> ops::Deref for ArrayQuery<T> {
    type Target = T;

    fn deref(&self) -> &T { &self.0 }
}

impl<T> ops::DerefMut for ArrayQuery<T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.0 }
}

impl<T: fmt::Display> fmt::Display for ArrayQuery<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.fmt(f) }
}

pub struct ArrayQueryConfig {
    #[allow(clippy::type_complexity)]
    err_handler: Option<Arc<dyn Fn(ArrayQueryPayloadError, &HttpRequest) -> Error + Send + Sync>>,
}

impl<T: DeserializeOwned> FromRequest for ArrayQuery<T> {
    type Error = Error;
    type Future = actix_utils::future::Ready<Result<Self, Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let error_handler = req.app_data::<ArrayQueryConfig>().and_then(|c| c.err_handler.clone());

        serde_qs::from_str::<T>(req.query_string())
            .map(|val| actix_utils::future::ok(ArrayQuery(val)))
            .unwrap_or_else(move |e| {
                let e = ArrayQueryPayloadError::Deserialize(e);

                log::debug!(
                    "Failed during Query extractor deserialization. Request path: {:?}",
                    req.path()
                );

                let e = if let Some(error_handler) = error_handler {
                    (error_handler)(e, req)
                } else {
                    e.into()
                };

                actix_utils::future::err(e)
            })
    }
}

/// this whole thing with string-gen could be static, since the format! only takes static args,
/// but it's not worth the effort, so we're using Into<String> and using heap-allocated,
/// copied-on-demand strings for this.
pub fn internal<T: Into<String>>(debuggable: impl Debug, e: T) -> HttpResponse {
    let e = e.into();
    // TODO(30): BUG: Check why the error logs are not showing up in tests (and if they'll show up
    // live)
    error!("{}:\n{:?}", e, debuggable);
    HttpResponse::InternalServerError().body(e)
}
