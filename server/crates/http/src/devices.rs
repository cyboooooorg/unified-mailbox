//! Device routes: pairing, listing, pairing codes and revocation.

use application::PairError;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use domain::{Device, DeviceId, DeviceNameError};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::CurrentDevice;
use crate::error::{ApiError, ErrorBody, ErrorCode};
use crate::extract::{ApiJson, ApiPath};
use crate::state::AppState;

// Not `Debug`: the code is a secret.
/// A pairing request from a new device.
#[derive(Deserialize, ToSchema)]
pub struct PairingRequest {
    /// The one-time pairing code, as typed: case, spaces and hyphens do
    /// not matter.
    pub code: String,
    /// The new device's name: trimmed, 1 to 64 characters, no control
    /// character.
    pub name: String,
}

// Not `Debug`: the token is a secret.
/// The new device and its bearer token, returned only here, only once.
#[derive(Serialize, ToSchema)]
pub struct PairingResponse {
    pub device_id: i64,
    /// `um_` then 43 base64url characters. Send it as
    /// `Authorization: Bearer <token>`.
    pub token: String,
}

/// A paired device that is not revoked.
#[derive(Debug, Serialize, ToSchema)]
pub struct DeviceDto {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
    /// Always present, `null` until the device is first seen. Updated at
    /// most once a minute.
    #[schema(required = true)]
    pub last_seen_at: Option<DateTime<Utc>>,
    /// Whether this is the device that made the request.
    pub current: bool,
}

impl DeviceDto {
    fn new(device: Device, caller: DeviceId) -> DeviceDto {
        DeviceDto {
            id: device.id.get(),
            current: device.id == caller,
            name: device.name.as_str().to_owned(),
            created_at: device.created_at,
            last_seen_at: device.last_seen_at,
        }
    }
}

// Not `Debug`: the code is a secret.
/// A one-time pairing code, to show to the person pairing a new device.
#[derive(Serialize, ToSchema)]
pub struct PairingCodeDto {
    /// `XXXX-XXXX`, Crockford base32.
    pub code: String,
    pub expires_at: DateTime<Utc>,
}

/// Pairs a new device with a one-time code; needs no token.
#[utoipa::path(post, path = "/pairing", operation_id = "pair_device", security(()),
    request_body = PairingRequest,
    responses(
        (status = 201, description = "Paired; the token is shown only here", body = PairingResponse),
        (status = 400, description = "Unknown, used or expired code", body = ErrorBody),
        (status = 413, description = "Body too large", body = ErrorBody),
        (status = 415, description = "Body is not JSON", body = ErrorBody),
        (status = 422, description = "Malformed body or invalid name", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    ))]
async fn pair(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<PairingRequest>,
) -> Result<(StatusCode, Json<PairingResponse>), ApiError> {
    let paired = state
        .pair_device
        .execute(&request.code, &request.name)
        .await
        .map_err(pair_error)?;
    Ok((
        StatusCode::CREATED,
        Json(PairingResponse {
            device_id: paired.device.id.get(),
            token: paired.token.as_str().to_owned(),
        }),
    ))
}

fn pair_error(err: PairError) -> ApiError {
    match err {
        PairError::InvalidCode => {
            ApiError::new(ErrorCode::InvalidPairingCode, "invalid pairing code")
        }
        PairError::InvalidName(err) => ApiError::new(ErrorCode::InvalidRequest, name_message(&err)),
        PairError::Repository(err) => err.into(),
    }
}

fn name_message(err: &DeviceNameError) -> String {
    match err {
        DeviceNameError::Empty => "name must not be empty".to_owned(),
        DeviceNameError::TooLong { max } => format!("name must be at most {max} characters"),
        DeviceNameError::ControlCharacter => "name must not contain a control character".to_owned(),
    }
}

/// Lists the devices not revoked, oldest first.
#[utoipa::path(get, path = "/devices", operation_id = "list_devices",
    responses(
        (status = 200, description = "The active devices", body = Vec<DeviceDto>),
        (status = 401, description = "Missing or invalid token", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    ))]
async fn list(
    CurrentDevice(caller): CurrentDevice,
    State(state): State<AppState>,
) -> Result<Json<Vec<DeviceDto>>, ApiError> {
    let devices = state.list_devices.execute().await?;
    Ok(Json(
        devices
            .into_iter()
            .map(|device| DeviceDto::new(device, caller))
            .collect(),
    ))
}

/// Creates a one-time pairing code, valid ten minutes.
#[utoipa::path(post, path = "/devices/pairing-codes", operation_id = "create_pairing_code",
    responses(
        (status = 201, description = "A new code", body = PairingCodeDto),
        (status = 401, description = "Missing or invalid token", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    ))]
async fn create_code(
    CurrentDevice(caller): CurrentDevice,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<PairingCodeDto>), ApiError> {
    let created = state.create_pairing_code.execute(Some(caller)).await?;
    Ok((
        StatusCode::CREATED,
        Json(PairingCodeDto {
            code: created.code.to_string(),
            expires_at: created.expires_at,
        }),
    ))
}

/// Revokes a device, the caller included: its token stops working.
#[utoipa::path(delete, path = "/devices/{id}", operation_id = "revoke_device",
    params(("id" = i64, Path, description = "Device id")),
    responses(
        (status = 204, description = "Revoked"),
        (status = 401, description = "Missing or invalid token", body = ErrorBody),
        (status = 404, description = "Unknown or already revoked", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    ))]
async fn revoke(
    CurrentDevice(_): CurrentDevice,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<i64>,
) -> Result<StatusCode, ApiError> {
    if state.revoke_device.execute(DeviceId::new(id)).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::new(ErrorCode::NotFound, "device not found"))
    }
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(pair))
        .routes(routes!(list))
        .routes(routes!(create_code))
        .routes(routes!(revoke))
}
