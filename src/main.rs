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
mod domain_nodes;
mod frps;
mod logging;
mod models;
mod node_services;
#[path = "redis.rs"]
mod redism;
mod schema;

use domain_nodes::*;
use frps::*;
use node_services::*;

static NOTFOUND: &[u8] = b"Not Found";

pub static NODE_LIVING_DURATION: u64 = 3 * 60;
pub static CROSS_COMPARE_INTERVAL: u64 = 60;

async fn health(_req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    Ok(Response::new(full(Bytes::from_static(b"ok"))))
}

async fn routers(req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    let response = match (req.method(), req.uri().path()) {
        (&Method::POST, path) if FRPS_PATH_RE.is_match(path) => handler(req).await,
        (&Method::POST, path) if NODE_API_PATH_RE.is_match(path) => node_api_handler(req).await,
        (&Method::GET, "/inner/nodes") => query_nodes(req).await,
        (&Method::GET, "/inner/living_nodes") => get_living_nodes(req).await,
        (&Method::GET, "/health-check") => health(req).await,
        (&Method::GET, "/domain_nodes") => get_domain_nodes(req).await,
        (&Method::PUT, "/domain_nodes") => create_domain_node(req).await,
        (&Method::DELETE, "/domain_nodes") => remove_domain_node(req).await,
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(full(NOTFOUND))
            .unwrap()),
    }?;

    Ok(response)
}

async fn close_expired_nodes(now: NaiveDateTime) {
    let expire_before = now
        .checked_sub_signed(chrono::Duration::seconds(NODE_LIVING_DURATION as i64))
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

async fn cross_compare_domain_nodes(_now: NaiveDateTime) {
    let start = chrono::Local::now().naive_local();

    let domains = db::get_distinct_domains().unwrap();
    for domain in domains {
        let nodes = db::get_nodes_by_domain(&domain).unwrap();
        let nodes_by_redis = redism::get_domain_nodes(&domain).unwrap();
        for node in nodes.iter() {
            if !nodes_by_redis.contains(&node) {
                log::error!("Node {} not found in redis for domain {}", node, domain);
                if let Err(e) = redism::nodes_join(&domain, vec![&node]) {
                    log::error!(
                        "Failed to add domain node to redis: {}. Error msg: {}",
                        domain,
                        e
                    );
                }
            }
        }
        for node in nodes_by_redis.iter() {
            if !nodes.contains(&node) {
                log::error!("Node {} not found in db for domain {}", node, domain);
                if let Err(e) = redism::node_lefts(&domain, &node) {
                    log::error!(
                        "Failed to del domain node in redis: {}. Error msg: {}",
                        domain,
                        e
                    );
                }
            }
        }
    }
    let end = chrono::Local::now().naive_local();
    // Log the time cost
    log::info!("Cross compare domain nodes finished in {:?}", end - start);
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
    cronjob(
        NODE_LIVING_DURATION,
        cluster,
        String::from("expiry_nodes_lock"),
        close_expired_nodes,
    )
    .await;

    // Cronjob for cross-comparing domain nodes
    cronjob(
        CROSS_COMPARE_INTERVAL,
        cluster,
        String::from("cross_compare_domain_nodes_lock"),
        cross_compare_domain_nodes,
    )
    .await;

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

async fn cronjob<F, Fut>(interval: u64, cluster: bool, lock_key: String, work: F)
where
    F: Fn(NaiveDateTime) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            let now = chrono::Local::now().naive_local();

            // Don't need redis if only run a single instance
            match cluster {
                true => {
                    match redism::establish_redis_conn() {
                        Ok(mut conn) => {
                            let opts = SetOptions::default()
                                .conditional_set(ExistenceCheck::NX)
                                .get(true)
                                .with_expiration(SetExpiry::EX(interval));

                            // Use distributed redis lock to avoid duplicate work.
                            // Set an expiration time for the lock to ensure that each attempt to acquire the lock will fail before the lock expires..
                            if let Ok(None) = conn.set_options::<&str, String, Option<String>>(
                                lock_key.as_str(),
                                now.to_string(),
                                opts,
                            ) {
                                work(now).await;
                            }
                        }
                        Err(e) => {
                            panic!("Failed to establish redis connection: {:?}", e);
                        }
                    }
                }
                false => {
                    work(now).await;
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
        }
    });
}
