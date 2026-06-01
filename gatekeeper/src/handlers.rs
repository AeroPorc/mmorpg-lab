use axum::{
    extract::State,
    http::StatusCode,
    Json,
};

use uuid::Uuid;

use crate::AppState;

use shared::*;

// Handle health requests -> just return "ok" to assert the server is alive
pub async fn health_handler() -> &'static str {
    "OK"
}

// Handle login requests -> ask redis for an available dedicated game server then return it to the player with a new player_id
pub async fn login_handler(
    State(_state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {

    // Authentification (pretty much here useless)
    if payload.username.is_empty() {
        println!("/!\\ New username is empty /!\\");
        return Err(StatusCode::UNAUTHORIZED);
    }

    if payload.password != "1234" {
        println!("/!\\ Invalid password /!\\");
        return Err(StatusCode::UNAUTHORIZED);
    }

    /*
    // Redis request
    let server =
        crate::redis_helper::find_available_server(&state.redis)
            .await
            .map_err(|err| {
                eprintln!("Redis error: {:?}", err);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let (ip, port, zone) = match server {
        Some(s) => s,
        None => {
            eprintln!("No available server found");
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
    };
    */

    // Response containing player_id and the dedicated game server
    Ok(Json(LoginResponse {
        player_id: Uuid::new_v4().to_string(),

        server: ServerInfo {
            ip: "127.0.0.1".to_string(),
            port: 5115,
            zone: "N/A".to_string(),
        },
    }))
}