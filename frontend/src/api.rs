use gloo_net::http::{Method, RequestBuilder, Headers};
use gloo_storage::{LocalStorage, Storage};
use serde::de::DeserializeOwned;
use thiserror::Error;

use serde::Serialize;
use log::error;


use model::entity::{EntryResponse, CreateEntryRequest, UpdateEntryRequest, LoginRequest, LoginResponse, EntryQuery, FindEntriesResponse, CategoryResponse, SourceResponse, CurrencyResponse, CreateCategoryRequest, EmptyResponse, UpdateCategoryRequest, CreateCurrencyRequest, UpdateCurrencyRequest, CreateSourceRequest, UpdateSourceRequest, CountResponse, CurrencyStatsResponse, CategoryStatsResponse};

use crate::state::State;

macro_rules! request_with_builder {
    ($url: expr, $method:expr, $builder_methods:expr) => {
        {
            let builder = RequestBuilder::new($url).method($method);

            let builder = $builder_methods(builder)?;

            let resp = builder.send()
                .await
                .map_err(|e| {
                    error!("Failed to send request to API client: {:?}", e);
                    ApiError::Unknown(e.to_string())
                })?;
            if resp.status() == 401 { return Err(ApiError::Unauthorized); }
            if !resp.ok() {
                return Err(ApiError::Http { status: resp.status(), body: resp.text().await.unwrap_or_default() });
            }
            resp.json::<T>().await.map_err(|e| ApiError::Decode(e.to_string()))
        }
    };
}

const TOKEN_KEY: &str = "jwt";

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("http {status}: {body}")]
    Http { status: u16, body: String },
    #[error("encode error: {0}")]
    Encode(String),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("unknown error: {0}")]
    Unknown(String),
}

pub struct ApiClient;

impl ApiClient {
    pub async fn login(req: LoginRequest) -> Result<LoginResponse, ApiError> {
        let resp = Self::post_json::<LoginRequest, LoginResponse>("/login", &req).await?;
        State::set_token(&resp.token);
        Ok(resp)
    }

    pub async fn find_entries(query: &EntryQuery) -> Result<FindEntriesResponse, ApiError> {
        let qs = serde_qs::to_string(query).map_err(|e| ApiError::Encode(e.to_string()))?;
        Self::get_json::<FindEntriesResponse>(&format!("/api/entry?{qs}")).await
    }

    pub async fn get_categories() -> Result<Vec<CategoryResponse>, ApiError> {
        Self::get_json::<Vec<CategoryResponse>>("/api/category").await
    }
    pub async fn get_category_stats(name: &str) -> Result<CategoryStatsResponse, ApiError> {
        Self::get_json::<CategoryStatsResponse>(&format!("/api/category/{name}/stats")).await
    }
    pub async fn get_category_entries(name: &str) -> Result<Vec<EntryResponse>, ApiError> {
        Self::get_json::<Vec<EntryResponse>>(&format!("/api/category/{name}/entries")).await
    }
    pub async fn get_category_by_name(name: &str) -> Result<CategoryResponse, ApiError> {
        Self::get_json::<CategoryResponse>(&format!("/api/category/{name}")).await
    }
    pub async fn create_category(req: &CreateCategoryRequest) -> Result<EmptyResponse, ApiError> {
        Self::post_json::<_, EmptyResponse>("/api/category", req).await
    }
    pub async fn update_category(name: &str, req: &UpdateCategoryRequest) -> Result<EmptyResponse, ApiError> {
        Self::post_json::<_, EmptyResponse>(&format!("/api/category/{name}"), req).await
    }
    pub async fn archive_category(name: &str) -> Result<EmptyResponse, ApiError> {
        Self::get_json::<EmptyResponse>(&format!("/api/category/{name}/archive")).await
    }

    pub async fn get_currencies() -> Result<Vec<CurrencyResponse>, ApiError> {
        Self::get_json::<Vec<CurrencyResponse>>("/api/currency").await
    }
    pub async fn get_currency_stats(name: &str) -> Result<CurrencyStatsResponse, ApiError> {
        Self::get_json::<CurrencyStatsResponse>(&format!("/api/currency/{name}/stats")).await
    }
    pub async fn get_currency_entries(name: &str) -> Result<Vec<EntryResponse>, ApiError> {
        Self::get_json::<Vec<EntryResponse>>(&format!("/api/currency/{name}/entries")).await
    }
    pub async fn get_currency_by_name(name: &str) -> Result<CurrencyResponse, ApiError> {
        Self::get_json::<CurrencyResponse>(&format!("/api/currency/{name}")).await
    }
    pub async fn get_currency_sources(name: &str) -> Result<Vec<SourceResponse>, ApiError> {
        Self::get_json::<Vec<SourceResponse>>(&format!("/api/currency/{name}/sources")).await
    }
    pub async fn create_currency(req: &CreateCurrencyRequest) -> Result<EmptyResponse, ApiError> {
        Self::post_json::<_, EmptyResponse>("/api/currency", req).await
    }
    pub async fn update_currency(name: &str, req: &UpdateCurrencyRequest) -> Result<EmptyResponse, ApiError> {
        Self::post_json::<_, EmptyResponse>(&format!("/api/currency/{name}"), req).await
    }
    pub async fn archive_currency(name: &str) -> Result<EmptyResponse, ApiError> {
        Self::get_json::<EmptyResponse>(&format!("/api/currency/{name}/archive")).await
    }

    pub async fn get_sources() -> Result<Vec<SourceResponse>, ApiError> {
        Self::get_json::<Vec<SourceResponse>>("/api/source").await
    }
    pub async fn get_source_entries(name: &str) -> Result<Vec<EntryResponse>, ApiError> {
        Self::get_json::<Vec<EntryResponse>>(&format!("/api/source/{name}/entries")).await
    }
    pub async fn get_source_by_name(name: &str) -> Result<SourceResponse, ApiError> {
        Self::get_json::<SourceResponse>(&format!("/api/source/{name}")).await
    }
    pub async fn create_source(req: &CreateSourceRequest) -> Result<EmptyResponse, ApiError> {
        Self::post_json::<_, EmptyResponse>("/api/source", req).await
    }
    pub async fn update_source(name: &str, req: &UpdateSourceRequest) -> Result<EmptyResponse, ApiError> {
        Self::post_json::<_, EmptyResponse>(&format!("/api/source/{name}"), req).await
    }
    pub async fn archive_source(name: &str) -> Result<EmptyResponse, ApiError> {
        Self::get_json::<EmptyResponse>(&format!("/api/source/{name}/archive")).await
    }

    pub async fn get_entries(page: Option<u32>) -> Result<Vec<EntryResponse>, ApiError> {
        let url = match page {
            Some(p) => format!("/api/entry/all?page={p}"),
            None => "/api/entry/all".to_string(),
        };
        Self::get_json::<Vec<EntryResponse>>(&url).await
    }
    pub async fn create_entry(req: &CreateEntryRequest) -> Result<EmptyResponse, ApiError> {
        Self::post_json::<_, EmptyResponse>("/api/entry", req).await
    }
    pub async fn delete_entries(ids: &[i32]) -> Result<CountResponse, ApiError> {
        Self::delete_json::<CountResponse>(&format!("/api/entry{}", Self::ids_qs(ids))).await
    }
    pub async fn archive_entries(ids: &[i32]) -> Result<CountResponse, ApiError> {
        Self::get_json::<CountResponse>(&format!("/api/entry/archive{}", Self::ids_qs(ids))).await
    }
    pub async fn update_entry(ids: &[i32], req: &UpdateEntryRequest) -> Result<CountResponse, ApiError> {
        let url = format!("/api/entry/update{}", Self::ids_qs(ids));
        Self::post_json::<_, CountResponse>(&url, req).await
    }

    fn ids_qs(ids: &[i32]) -> String {
        if ids.is_empty() { return String::new(); }
        let mut s = String::from("?");
        for (i, id) in ids.iter().enumerate() {
            if i > 0 { s.push('&'); }
            s.push_str(&format!("ids[]={}", id));
        }
        s
    }

    fn headers() -> Headers {
        let h = Headers::new();
        if let Some(t) = State::get_token() {
            h.set("Authorization", &format!("Bearer {}", t));
        }
        h
    }

    async fn delete_json<T: DeserializeOwned>(url: &str) -> Result<T, ApiError> {
        request_with_builder!(url, Method::DELETE, |b: RequestBuilder|
            Ok(b.headers(Self::headers())))
    }

    async fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, ApiError> {
        request_with_builder!(url, Method::GET, |b: RequestBuilder|
            Ok(b.headers(Self::headers())))
    }

    async fn post_json<B: Serialize, T: DeserializeOwned>(url: &str, body: &B) -> Result<T, ApiError> {
        request_with_builder!(url, Method::POST, |b: RequestBuilder|
            b.headers(Self::headers()).json(body).map_err(|e| ApiError::Encode(e.to_string())))
    }
}
