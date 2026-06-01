mod handlers;
//mod redis_helper;

use shared::ServerInfo;

use axum::{
    routing::{get, post},
    Router,
};

use std::error::Error;

#[derive(Clone)]
pub struct AppState {
    redis: redis::Client,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create a redis client at the given URL "redis://127.0.0.1/"
    let redis = match redis::Client::open("redis://127.0.0.1/") {
        Ok(client) => {
            println!("Connexion Redis initialisée");
            client
        }
        Err(err) => {
            eprintln!("Impossible de créer le client Redis: {}", err);
            return Err(err.into());
        }
    };

    let state = AppState { redis };

    // Create a router to handle LoginRequest and others through handlers
    let gatekeeper_server = ServerInfo {
        ip: String::from("0.0.0.0"),
        port: 3000,
        zone: String::from("N/A"),
    };

    let app = Router::new()
        .route("/health", get(handlers::health_handler)) // Assert the server is alive
        .route("/login", post(handlers::login_handler)) // Handle login
        .with_state(state);

    // Start the server (TcpListener)
    let listener = match tokio::net::TcpListener::bind(gatekeeper_server.base()).await {
        Ok(listener) => {
            println!("Gatekeeper lancé sur {}", gatekeeper_server.base());
            listener
        }
        Err(err) => {
            eprintln!("Impossible de bind {}: {}", gatekeeper_server.base(), err);
            return Err(err.into());
        }
    };

    if let Err(err) = axum::serve(listener, app).await {
        eprintln!("Erreur serveur Axum: {}", err);
        return Err(err.into());
    }

    Ok(())
}