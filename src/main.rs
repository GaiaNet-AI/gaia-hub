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
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

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
pub static CHECKING_NODES_HEALTH_DURATION: u64 = 60 * 60;

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
    match db::unavail_expired_nodes(&expire_before) {
        Ok(n) => {
            log::info!("Made {} expired nodes unavail", n);
        }
        Err(err) => {
            log::error!("Failed to unavail expired nodes: {:?}", err);
        }
    }
    match db::close_expired_nodes(&expire_before) {
        Ok(n) => {
            log::info!("Closed {} expired nodes", n);
        }
        Err(err) => {
            log::error!("Failed to close expired nodes: {:?}", err);
        }
    }
}

async fn check_nodes_health(_now: NaiveDateTime) {
    let mut earliest_login_time = chrono::DateTime::from_timestamp(0, 0)
        .unwrap()
        .naive_utc()
        .and_utc()
        .timestamp();

    let least_lived_secs = 10;
    let page_size = 100;
    let request_timeout_secs = 5;
    // Limit the number of concurrent tasks
    let semaphore = Arc::new(Semaphore::new(10));

    loop {
        let nodes =
            db::query_living_nodes_by_login_time(least_lived_secs, page_size, earliest_login_time);
        if let Err(_) = nodes {
            break;
        }
        let nodes = nodes.unwrap();

        let len = nodes.len() as i64;

        if len == 0 {
            break;
        }

        // Next loop, check the node that login after this time.
        earliest_login_time = nodes[nodes.len() - 1].login_time;

        for node in nodes {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            tokio::spawn(async move {
                // Perform health check for the node
                let is_healthy = determine_node_avail(&node, request_timeout_secs).await;
                if !is_healthy {
                    log::info!("Make node {} unavail because it is unhealthy", node.node_id);
                    let _ = db::unavail_node(&node.node_id);
                }
                // Release the permit after the task is done
                drop(permit);
            });
        }

        if len < page_size {
            break;
        }
    }
}

// Determine the node health based on whether we can get successful response from the node
async fn determine_node_avail(node: &models::LivingNode, timeout: u64) -> bool {
    let client = reqwest::Client::new();

    client
        .post(format!("https://{}/v1/chat/completions", node.subdomain))
        .header("Accept", "text/event-stream")
        .json(&serde_json::json!({
            "messages": [
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "Hello"},
            ],
            "model": node.chat_model,
            "stream": true
        }))
        .timeout(std::time::Duration::from_secs(timeout))
        .send()
        .await
        .map(|res| res.status().is_success())
        .unwrap_or(true)
}

async fn cross_compare_domain_nodes(_now: NaiveDateTime) {
    let start = chrono::Utc::now().naive_utc();

    let domains = db::get_distinct_domains().unwrap();
    for domain in domains {
        let nodes = db::get_nodes_by_domain(&domain).unwrap();
        let nodes_by_redis = redism::get_domain_nodes(&domain).unwrap();
        for node in nodes.iter() {
            if !nodes_by_redis.contains(&node) {
                log::error!("Node {} not found in redis for domain {}", node.0, domain);
                if let Err(e) = redism::nodes_join(&domain, &node.0, node.1) {
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
                log::error!("Node {} not found in db for domain {}", node.0, domain);
                if let Err(e) = redism::node_lefts(&domain, &node.0, node.1) {
                    log::error!(
                        "Failed to del domain node in redis: {}. Error msg: {}",
                        domain,
                        e
                    );
                }
            }
        }
    }
    let end = chrono::Utc::now().naive_utc();
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
        NODE_LIVING_DURATION,
        close_expired_nodes,
    )
    .await;

    // Cronjob for checking nodes health
    // Pass 1 min as the interval because the lock duration is long enough
    cronjob(
        60,
        cluster,
        String::from("check_nodes_health_lock"),
        CHECKING_NODES_HEALTH_DURATION,
        check_nodes_health,
    )
    .await;

    // Cronjob for cross-comparing domain nodes
    cronjob(
        CROSS_COMPARE_INTERVAL,
        cluster,
        String::from("cross_compare_domain_nodes_lock"),
        CROSS_COMPARE_INTERVAL,
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

async fn cronjob<F, Fut>(
    interval: u64,
    cluster: bool,
    lock_key: String,
    lock_duration: u64,
    work: F,
) where
    F: Fn(NaiveDateTime) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            let now = chrono::Utc::now().naive_utc();

            // Don't need redis if only run a single instance
            match cluster {
                true => {
                    match redism::establish_redis_conn() {
                        Ok(mut conn) => {
                            let opts = SetOptions::default()
                                .conditional_set(ExistenceCheck::NX)
                                .get(true)
                                .with_expiration(SetExpiry::EX(lock_duration));

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
