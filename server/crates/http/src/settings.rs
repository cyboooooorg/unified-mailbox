//! Settings routes: the language of the user.

use axum::Json;
use axum::extract::State;
use domain::Language;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::CurrentDevice;
use crate::error::{ApiError, ErrorBody};
use crate::extract::ApiJson;
use crate::state::AppState;

/// A language the interface and Claude's text can be in.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LanguageDto {
    En,
    Fr,
}

impl From<Language> for LanguageDto {
    fn from(language: Language) -> Self {
        match language {
            Language::En => LanguageDto::En,
            Language::Fr => LanguageDto::Fr,
        }
    }
}

impl From<LanguageDto> for Language {
    fn from(language: LanguageDto) -> Self {
        match language {
            LanguageDto::En => Language::En,
            LanguageDto::Fr => Language::Fr,
        }
    }
}

/// The language the user chose.
#[derive(Debug, Serialize, ToSchema)]
pub struct LanguageSettingDto {
    /// Always present, `null` until the user chooses one; the client then
    /// follows its own locale.
    #[schema(required = true)]
    pub language: Option<LanguageDto>,
}

/// The language to choose.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LanguageUpdateDto {
    pub language: LanguageDto,
}

/// Returns the language the user chose, `null` when they have not.
#[utoipa::path(get, path = "/settings/language", operation_id = "get_language",
    responses(
        (status = 200, description = "The chosen language", body = LanguageSettingDto),
        (status = 401, description = "Missing or invalid token", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    ))]
async fn get_language(
    CurrentDevice(_): CurrentDevice,
    State(state): State<AppState>,
) -> Result<Json<LanguageSettingDto>, ApiError> {
    let language = state.get_language.execute().await?;
    Ok(Json(LanguageSettingDto {
        language: language.map(Into::into),
    }))
}

/// Chooses the language; every client is told through the event stream.
#[utoipa::path(put, path = "/settings/language", operation_id = "set_language",
    request_body = LanguageUpdateDto,
    responses(
        (status = 200, description = "The language now chosen", body = LanguageSettingDto),
        (status = 401, description = "Missing or invalid token", body = ErrorBody),
        (status = 413, description = "Body too large", body = ErrorBody),
        (status = 415, description = "Body is not JSON", body = ErrorBody),
        (status = 422, description = "Malformed body or unknown language", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    ))]
async fn set_language(
    CurrentDevice(_): CurrentDevice,
    State(state): State<AppState>,
    ApiJson(update): ApiJson<LanguageUpdateDto>,
) -> Result<Json<LanguageSettingDto>, ApiError> {
    let language = Language::from(update.language);
    state.set_language.execute(language).await?;
    Ok(Json(LanguageSettingDto {
        language: Some(language.into()),
    }))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_language, set_language))
}
