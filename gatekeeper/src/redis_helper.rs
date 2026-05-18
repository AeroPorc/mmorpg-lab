use redis::AsyncCommands;

pub async fn find_available_server(
    client: &redis::Client,
) -> anyhow::Result<Option<(String, u16, String)>> {
    let mut conn = client.get_multiplexed_async_connection().await?;

    let keys: Vec<String> = conn.keys("server:*").await?;

    for key in keys {
        let ip: String = conn.hget(&key, "ip").await?;
        let port: u16 = conn.hget(&key, "port").await?;
        let zone: String = conn.hget(&key, "zone").await?;

        return Ok(Some((ip, port, zone)));
    }

    Ok(None)
}