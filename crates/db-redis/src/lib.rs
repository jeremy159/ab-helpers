use fred::prelude::*;

pub type RedisPool = fred::clients::RedisPool;

pub async fn get_connection_pool(url: &str) -> Result<RedisPool, RedisError> {
    let config = RedisConfig::from_url(url)?;
    let pool = Builder::from_config(config)
        .with_performance_config(|config| {
            config.auto_pipeline = true;
        })
        .build_pool(
            std::env::var("REDIS_POOL_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
        )?;

    pool.init().await?;
    Ok(pool)
}
