#![deny(warnings)]

use bytes::Bytes;
use chrono::NaiveDateTime;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{body::Incoming as IncomingBody, Method, Request, Response, StatusCode};

use gaia_hub::*;
use hyper_util::rt::TokioIo;
use redis::{Commands, ExistenceCheck, SetExpiry, SetOptions};
use std::env;
use tokio::net::TcpListener;

mod args;
mod db;
mod frps;
mod logging;
mod models;
mod node_services;
#[path = "redis.rs"]
mod redism;
mod schema;

use frps::{get_living_nodes, handler, query_nodes, FRPS_PATH_RE};
use node_services::{node_api_handler, NODE_API_PATH_RE};

static NOTFOUND: &[u8] = b"Not Found";

pub static NODE_LIVING_DURATION: i64 = 3 * 60;

async fn health(_req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    Ok(Response::new(full(Bytes::from_static(b"ok"))))
}

async fn routers(req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    let response = match (req.method(), req.uri().path()) {
        (&Method::POST, path) if FRPS_PATH_RE.is_match(path) => handler(req).await,
        (&Method::POST, path) if NODE_API_PATH_RE.is_match(path) => node_api_handler(req).await,
        (&Method::GET, "/handler/nodes") => query_nodes(req).await,
        (&Method::GET, "/handler/living_nodes") => get_living_nodes(req).await,
        (&Method::GET, "/health-check") => health(req).await,
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(full(NOTFOUND))
            .unwrap()),
    }?;

    Ok(response)
}

async fn close_expired_nodes(now: NaiveDateTime) {
    let expire_before = now
        .checked_sub_signed(chrono::Duration::seconds(NODE_LIVING_DURATION))
        .unwrap();
    match db::close_expired_nodes(&expire_before) {
        Ok(n) => {
            log::info!("Closed {} expired nodes", n);
        }
        Err(err) => {
            log::error!("Failed to close expired nodes: {:?}", err);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Trigger the args parsing
    let cluster = crate::args::ARGS.cluster;

    logging::configure_logging();

    let host = env::var("SERVER_HOST").expect("No SERVER_HOST in env");
    let port = env::var("SERVER_PORT").expect("No SERVER_PORT in env");

    let addr = format!("{}:{}", host, port);

    let listener: TcpListener = TcpListener::bind(&addr).await?;
    log::info!("Listening on http://{}", addr);

    // Cronjob for closing expired nodes
    tokio::spawn(async move {
        loop {
            let now = chrono::Local::now().naive_local();

            // Don't need redis if only run a single instance
            match cluster {
                true => {
                    if let Ok(mut conn) = redism::establish_redis_conn() {
                        let opts = SetOptions::default()
                            .conditional_set(ExistenceCheck::NX)
                            .get(true)
                            .with_expiration(SetExpiry::EX(NODE_LIVING_DURATION as u64));

                        // Use distributed redis lock to avoid duplicate work.
                        // Set an expiration time for the lock to ensure that each attempt to acquire the lock will fail before the lock expires..
                        if let Ok(None) = conn.set_options::<&str, String, Option<String>>(
                            "expiry_nodes_lock",
                            now.to_string(),
                            opts,
                        ) {
                            close_expired_nodes(now).await;
                        }
                    }
                }
                false => {
                    close_expired_nodes(now).await;
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(
                NODE_LIVING_DURATION as u64,
            ))
            .await;
        }
    });

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            let service = service_fn(move |req| routers(req));
            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                log::info!("Failed to serve connection: {:?}", err);
            }
        });
    }
}
