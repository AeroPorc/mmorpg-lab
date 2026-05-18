use shared::*;
/*
async fn login_to_gatekeeper(
    username: &str,
    password: &str,
) -> Result<LoginResponse, Box<dyn std::error::Error>> {
    let base_url = "http://127.0.0.1:3000";

    let client = reqwest::Client::new();

    // POST /login (vrai login)
    let response = client
        .post(format!("{}/login", base_url))
        .json(&LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        })
        .send()
        .await?
        .json::<LoginResponse>()
        .await?;

    println!("Logged in successfully!");
    println!("Token: {}", response.player_id);
    println!(
        "Game server: {}.{} -> {}",
        response.server.ip, response.server.port, response.server.zone
    );

    Ok(response)
}
*/
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gatekeeper_server = ServerInfo {
        ip: String::from("127.0.0.1"),
        port: 3000,
        zone: String::from("N/A"),
    };

    // Requête GET /health
    println!("Is Gatekeeper alive ?");

    let response = reqwest::get(gatekeeper_server.http_url("/health"))
        .await?
        .text()
        .await?;

    println!("Response from gatekeeper: {}", response);

    // POST /login (vrai login)
    let client = reqwest::Client::new();

    let response = client
        .post(gatekeeper_server.http_url("/login"))
        .json(&LoginRequest {
            username: String::from("Pierre"),
            password: String::from("1234"),
        })
        .send()
        .await?
        .json::<LoginResponse>()
        .await?;

    println!("Logged in successfully!");
    println!("player_id: {}", response.player_id);
    println!(
        "Game server: {}.{} -> {}",
        response.server.ip, response.server.port, response.server.zone
    );

    Ok(())
}