use bytes::{Buf, Bytes};
use gaia_hub::*;
use http_body_util::BodyExt;
use hyper::{body::Incoming as IncomingBody, Request, Response};
use lazy_static::lazy_static;
use log;
use regex::Regex;

use crate::db::*;

lazy_static! {
    pub(crate) static ref DEVICE_API_PATH_RE: Regex =
        Regex::new(r"^/(?<path>(?:device-info)|(?:device-health))/(?<device_id>[\w\.\-]+)?$")
            .unwrap();
}

pub async fn device_api_handler(req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    let captures = DEVICE_API_PATH_RE
        .captures(req.uri().path())
        .ok_or("Invalid path")?;
    let path = captures.name("path").and_then(|m| Some(m.as_str()));
    let device_id = captures.name("device_id").and_then(|m| Some(m.as_str()));

    let device_id = device_id.ok_or("Invalid device_id")?.to_string();

    match path {
        Some("device-info") => device_info_handler(device_id, req).await,
        Some("device-health") => device_health_handler(device_id, req).await,
        _ => Err("Invalid path".into()),
    }
}

async fn device_health_handler(
    device_id: String,
    req: Request<IncomingBody>,
) -> Result<Response<BoxBody>> {
    // Aggregate the body...
    let whole_body = req.collect().await?.aggregate();

    // Decode as JSON...
    let device_health: serde_json::Value = serde_json::from_reader(whole_body.reader())?;

    let health = device_health["health"]
        .as_bool()
        .ok_or("No health attribute")?;

    let nodes = query_nodes_by_device_id(&device_id)?;
    match health {
        true => {
            let now = chrono::Utc::now().naive_utc();
            for node in nodes {
                if node.status == NODE_STATUS_ONLINE {
                    // Update the last avail time
                    update_node_avail_time_and_status(&node.node_id, &now, NODE_STATUS_ONLINE)?;
                } else if node.status == NODE_STATUS_UNAVAIL {
                    // Reopen the avail node
                    // while frpc is connected by checking last_active_time
                    let active_after = now
                        .checked_sub_signed(chrono::Duration::seconds(
                            crate::NODE_LIVING_DURATION as i64,
                        ))
                        .unwrap();
                    if node.last_active_time > active_after.and_utc().timestamp() {
                        update_node_avail_time_and_status(&node.node_id, &now, NODE_STATUS_ONLINE)?;
                    }
                }
            }
        }
        false => {
            // Unavail the nodes of device
            update_nodes_status_by_device_id(&device_id, NODE_STATUS_UNAVAIL)?;
        }
    }

    Ok(Response::new(crate::full(Bytes::from_static(b"ok"))))
}

async fn device_info_handler(
    device_id: String,
    req: Request<IncomingBody>,
) -> Result<Response<BoxBody>> {
    // Aggregate the body...
    let whole_body = req.collect().await?.aggregate();

    // Decode as JSON...
    let node_info: serde_json::Value = serde_json::from_reader(whole_body.reader())?;

    let node_version = node_info["node_version"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let chat_model_name = node_info["chat_model"]["name"]
        .as_str()
        .ok_or("Missing chat_model in node info")?
        .to_string();
    let embedding_model_name = node_info["embedding_model"]["name"]
        .as_str()
        .ok_or("Missing embedding_model in node info")?
        .to_string();

    update_nodes_info_by_device_id(
        &device_id,
        &node_version,
        &chat_model_name,
        &embedding_model_name,
    )?;
    log::info!(
        "Updated nodes info of device {}: node_version: {}, chat model name: {}, embedding model name: {}",
        device_id,
        node_version,
        chat_model_name,
        embedding_model_name
    );

    Ok(Response::new(crate::full(Bytes::from_static(b"ok"))))
}
