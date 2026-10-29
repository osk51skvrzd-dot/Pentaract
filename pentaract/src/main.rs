
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use tokio::{sync::mpsc, time};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    common::{channels::ClientMessage, db::pool::get_pool, routing::app_state::AppState},
    config::Config,
    server::Server,
    startup::{create_superuser, init_db},
    storage_manager::StorageManager,
};

mod common;
mod config;
mod errors;
mod models;
mod repositories;
mod routers;
mod schemas;
mod server;
mod services;
mod startup;
mod storage_manager;

#[tokio::main]
async fn main() {
    let config = Config::new().unwrap();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "pentaract=info,tower_http=warn,axum::rejection=warn".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let (tx, rx) = mpsc::channel::<ClientMessage>(
        usize::from(config.channel_capacity).clamp(1, 16),
    );

    let db = get_pool(
        &config.db_uri,
        u32::from(config.workers),
        time::Duration::from_secs(30),
    )
    .await;

    init_db(&db).await;
    create_superuser(&db, &config).await;

    let manager_db = db.clone();
    let manager_config = config.clone();

    tokio::spawn(async move {
        let mut manager =
            StorageManager::new(rx, manager_db, manager_config);

        tracing::info!("StorageManager started");
        manager.run().await;
    });

    let addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        config.port,
    );

    let workers = usize::from(config.workers).clamp(1, 2);
    let app_state = AppState::new(db, config, tx);
    let shared_state = Arc::new(app_state);

    let server = Server::build_server(workers, shared_state);
    server.run(&addr).await;
}
