use crate::{
    config::{
        env::{domain, secret, use_https},
        DatabaseSettings,
        Settings,
    },
    context::AppContext,
    user_service::router as user,
    workspace_service::{app::router as app, view::router as view, workspace::router as workspace},
    ws_service,
    ws_service::WSServer,
};
use actix::Actor;
use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_web::{dev::Server, middleware, web, web::Data, App, HttpServer, Scope};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{net::TcpListener, time::Duration};
use tokio::time::interval;

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(configuration: Settings) -> Result<Self, std::io::Error> {
        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );
        let listener = TcpListener::bind(&address)?;
        let port = listener.local_addr().unwrap().port();
        let app_ctx = init_app_context(&configuration).await;
        let server = run(listener, app_ctx)?;
        Ok(Self { port, server })
    }

    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> { self.server.await }

    pub fn port(&self) -> u16 { self.port }
}

pub fn run(listener: TcpListener, app_ctx: AppContext) -> Result<Server, std::io::Error> {
    let AppContext { ws_server, pg_pool } = app_ctx;
    let ws_server = Data::new(ws_server);
    let pg_pool = Data::new(pg_pool);
    let domain = domain();
    let secret: String = secret();

    actix_rt::spawn(period_check(pg_pool.clone()));

    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(identify_service(&domain, &secret))
            .wrap(crate::middleware::default_cors())
            .wrap(crate::middleware::AuthenticationService)
            .app_data(web::JsonConfig::default().limit(4096))
            .service(ws_scope())
            .service(user_scope())
            .app_data(ws_server.clone())
            .app_data(pg_pool.clone())
    })
    .listen(listener)?
    .run();
    Ok(server)
}

async fn period_check(_pool: Data<PgPool>) {
    let mut i = interval(Duration::from_secs(60));
    loop {
        i.tick().await;
    }
}

fn ws_scope() -> Scope { web::scope("/ws").service(ws_service::router::start_connection) }

fn user_scope() -> Scope {
    // https://developer.mozilla.org/en-US/docs/Web/HTTP
    // TODO: replace GET body with query params
    web::scope("/api")
        // authentication
        .service(web::resource("/auth")
            .route(web::post().to(user::sign_in_handler))
            .route(web::delete().to(user::sign_out_handler))
        )
        .service(web::resource("/user")
            .route(web::patch().to(user::set_user_profile_handler))
            .route(web::get().to(user::get_user_profile_handler))
        )
        .service(web::resource("/register")
            .route(web::post().to(user::register_handler))
        )
        .service(web::resource("/workspace")
            .route(web::post().to(workspace::create_handler))
            .route(web::delete().to(workspace::delete_handler))
            .route(web::get().to(workspace::read_handler))
            .route(web::patch().to(workspace::update_handler))
        )
        .service(web::resource("/workspace_list/{user_id}")
            .route(web::get().to(workspace::workspace_list))
        )
        .service(web::resource("/app")
            .route(web::post().to(app::create_handler))
            .route(web::delete().to(app::delete_handler))
            .route(web::get().to(app::read_handler))
            .route(web::patch().to(app::update_handler))
        )
        .service(web::resource("/view")
            .route(web::post().to(view::create_handler))
            .route(web::delete().to(view::delete_handler))
            .route(web::get().to(view::read_handler))
            .route(web::patch().to(view::update_handler))
        )
        // password
        .service(web::resource("/password_change")
            .route(web::post().to(user::change_password))
        )
}

async fn init_app_context(configuration: &Settings) -> AppContext {
    let _ = flowy_log::Builder::new("flowy").env_filter("Debug").build();
    let pg_pool = get_connection_pool(&configuration.database)
        .await
        .expect(&format!(
            "Failed to connect to Postgres at {:?}.",
            configuration.database
        ));

    let ws_server = WSServer::new().start();

    AppContext::new(ws_server, pg_pool)
}

pub fn identify_service(domain: &str, secret: &str) -> IdentityService<CookieIdentityPolicy> {
    IdentityService::new(
        CookieIdentityPolicy::new(secret.as_bytes())
            .name("auth")
            .path("/")
            .domain(domain)
            .max_age_secs(24 * 3600)
            .secure(use_https()),
    )
}

pub async fn get_connection_pool(configuration: &DatabaseSettings) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .connect_timeout(std::time::Duration::from_secs(5))
        .connect_with(configuration.with_db())
        .await
}
