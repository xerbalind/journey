use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::routes::{create_point, create_temperature, get_geojsons, get_position};
use axum::{
    Router,
    routing::{get, post},
};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/");

mod error;
mod geojson;
mod models;
mod routes;
mod schema;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL environment is missing");
    let manager = deadpool_diesel::postgres::Manager::new(db_url, deadpool_diesel::Runtime::Tokio1);
    let pool = deadpool_diesel::postgres::Pool::builder(manager)
        .build()
        .expect("could not create a postgres pool");

    {
        let conn = pool.get().await.unwrap();
        conn.interact(|conn| conn.run_pending_migrations(MIGRATIONS).map(|_| ()))
            .await
            .unwrap()
            .unwrap();
    }

    let cors = CorsLayer::permissive();

    let mut app = Router::new()
        .route("/api/journey.geojsons", get(get_geojsons))
        .route("/api/position", get(get_position))
        .route("/api/point", post(create_point))
        .route("/api/temperature", post(create_temperature))
        .layer(cors)
        .with_state(pool);

    app = app.fallback_service(ServeDir::new("./public"));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let _ = axum::serve(listener, app).await;
}
