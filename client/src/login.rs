use shared::*;
use bevy::prelude::*;

#[derive(Resource, Clone)]
pub struct GatekeeperInfo(pub ServerInfo);

impl GatekeeperInfo {
    pub fn login_url(&self) -> String {
        self.0.http_url("/login")
    }

    pub fn health_url(&self) -> String {
        self.0.http_url("/health")
    }
}

/*
pub struct GatekeeperLoginPlugin;

impl Plugin for GatekeeperLoginPlugin {
    fn build(&self, app: &mut App) {
        app//.init_resource::<AuthState>()
            .insert_resource(GatekeeperInfo { 
                0: ServerInfo {
                    ip: "127.0.0.1".to_string(),
                    port: 3000,
                    zone: "N/A".to_string(),
                },
            })
            .add_systems(OnEnter(AppState::Login), start_login);
            //.add_systems(Update, poll_login_task.run_if(in_state(AppState::Login)));
            //.add_systems(Startup, start_login);
    }
}

/*
fn start_login(gatekeeper: Res<GatekeeperInfo>) {

    std::thread::spawn(move || {
    // Requête GET /health
    println!("Is Gatekeeper alive ?");

    let response = reqwest::get(gatekeeper.health_url()).await?.text().await?;

    println!("Response from gatekeeper: {}", response);

    // POST /login (vrai login)
    let client = reqwest::Client::new();

    let response = client
        .post(gatekeeper.login_url())
        .json(&LoginRequest {
            username: String::from("Pierre"),
            password: String::from("1234"),
        })
        .send()?
        .json::<LoginResponse>()?;

    println!("Logged in successfully!");
    println!("player_id: {}", response.player_id);
    println!(
        "Game server: {}.{} -> {}",
        response.server.ip, response.server.port, response.server.zone
    );
});
}
*/

use bevy::tasks::AsyncComputeTaskPool;

fn start_login(
    gatekeeper: Res<GatekeeperInfo>,
    mut commands: Commands,
) {
    let pool = AsyncComputeTaskPool::get();
    let gate = gatekeeper.clone();

    let task = pool.spawn(async move {
        let client = reqwest::Client::new();

        // 🔍 health check
        let _ = reqwest::get(gate.health_url())
            .await
            .ok()
            .and_then(|r| futures_lite::future::block_on(r.text()).ok());

        // 🔐 login
        let response = client
            .post(gate.login_url())
            .json(&LoginRequest {
                username: "Pierre".into(),
                password: "1234".into(),
            })
            .send()
            .await
            .ok()?
            .json::<LoginResponse>()
            .await
            .ok()?;

        Some(response)
    });

    commands.spawn(LoginTask(task));
}

fn poll_login_task(
    mut commands: Commands,
    mut query: Query<(Entity, &mut LoginTask)>,
    //mut auth: ResMut<AuthState>,
    mut game_server: ResMut<GameServerInfo>,
    mut next: ResMut<NextState<AppState>>,
) {
    for (entity, mut task) in &mut query {
        if let Some(Some(res)) =
            bevy::tasks::block_on(futures_lite::future::poll_once(&mut task.0))
        {
            // 🔐 auth
            auth.player_id = Some(res.player_id.clone());

            // 🎮 game server
            *game_server = GameServerInfo {
                server: ServerInfo {
                    ip: res.server.ip,
                    port: res.server.port,
                    zone: res.server.zone,
                },
            };

            println!("Login OK: {}", res.player_id);

            commands.entity(entity).despawn();

            // 👉 transition state
            next.set(AppState::Connecting);
        }
    }
}
*/