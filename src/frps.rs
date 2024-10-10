#![allow(warnings)]

use bytes::{Buf, Bytes};
use http_body_util::BodyExt;
use hyper::{body::Incoming as IncomingBody, header, Request, Response, StatusCode};
use log::{info, warn};
use std::collections::HashMap;

use crate::db::*;
use gaia_hub::*;
use serde_json::Value;

use lazy_static::lazy_static;

use regex::Regex;

lazy_static! {
    pub(crate) static ref FRPS_PATH_RE: Regex =
        Regex::new(r"^/inner/frps(?:/(?<id>frps_\d+))?$").unwrap();
}

fn process_json_data_and_build_response(data: serde_json::Value) -> Result<Response<BoxBody>> {
    let mut modified_data = data;

    // Modify the JSON data...
    modified_data["reject"] = serde_json::Value::Bool(false);
    modified_data["unchange"] = serde_json::Value::Bool(true);

    // Log the modified JSON...
    let formatted_json = serde_json::to_string(&modified_data)?;

    // Construct the response...
    let json = serde_json::to_string(&modified_data)?;
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(full(json))?;

    Ok(response)
}

pub async fn handler(req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    let path = req.uri().path().to_owned();
    let frps_id = FRPS_PATH_RE
        .captures(&path)
        .and_then(|caps| caps.name("id"))
        .and_then(|m| Some(m.as_str()));

    let whole_body = req.collect().await?.aggregate();
    let mut data: serde_json::Value = serde_json::from_reader(whole_body.reader())?;
    let response = process_json_data_and_build_response(data.clone())?;

    if let Err(e) = handler_inner(frps_id, &data).await {
        log::error!("Failed to handle request: {}", e);
    }
    Ok(response)
}

async fn handler_inner(frps_id: Option<&str>, data: &serde_json::Value) -> Result<()> {
    let op = data["op"].as_str().unwrap();
    if op != "Ping" {
        let formatted_json = serde_json::to_string(data).expect("Failed to serialize JSON");
        log::info!("Received: {}", formatted_json.replace(r#"""#, r#"\""#));
    }

    // Handle JSON data and construct response...

    if op == "Login" {
        let content = &data["content"];
        let metas = &content["metas"];
        let device_id = match metas["deviceId"].as_str() {
            Some(id) => id,
            None => Err(format!("Device ID not found in metas: {:?}", metas))?,
        };
        if device_id.is_empty() {
            Err("Device ID is empty")?
        }
        let os = content["os"].as_str().unwrap_or("default_os");
        let arch = content["arch"].as_str().unwrap_or("default_arch");
        let version = content["version"].as_str().unwrap_or("0.0.0");
        let client_address = match content["client_address"].as_str() {
            Some(addr) => addr,
            None => Err(format!(
                "Client address not found in content: {:?}",
                content
            ))?,
        };

        let login_time = chrono::Utc::now().naive_utc();

        let count = count_device_by_device_id(device_id)?;
        if count == 0 {
            let _device = create_device(
                device_id,
                os,
                arch,
                version,
                client_address,
                &metas,
                &login_time,
            )?;
        } else {
            // Update the login_time of device
            update_device(device_id, &login_time)?;
        }
    } else if op == "NewProxy" {
        let content = &data["content"];
        let metas = &content["user"]["metas"];
        let device_id = match metas["deviceId"].as_str() {
            Some(id) => id,
            None => Err(format!("Device ID not found in metas: {:?}", metas))?,
        };
        if device_id.is_empty() {
            Err("Device ID is empty")?
        }
        let node_id = match content["subdomain"].as_str() {
            Some(id) => id,
            None => Err(format!("Node ID not found in content: {:?}", content))?,
        };
        let run_id = content["user"]["run_id"].as_str().unwrap();
        let subdomain = match content["proxy_name"].as_str() {
            Some(id) => id,
            None => Err(format!("Subdomain not found in content: {:?}", content))?,
        };

        // Record the subdomain and frps_id mapping in redis
        if let Some(frps_id) = frps_id {
            if let Err(e) = crate::redism::set_subdomain_frps_id(subdomain, frps_id) {
                log::error!(
                    "Failed to set redis key/value: {}/{}. Error msg: {}",
                    subdomain,
                    frps_id,
                    e
                );
            }
        }

        let now = chrono::Utc::now().naive_utc();

        let devices = query_device_by_device_id(device_id)?;

        if devices.is_empty() {
            Err(format!("Device not found: {}", device_id))?
        }

        let device = &devices[0];
        let last_login_time = device.login_time;

        let client_address = device.client_address.clone();
        let os = device.os.clone();
        let arch = device.arch.clone();
        let version = device.version.clone();

        let node = query_node_by_node_id(node_id)?;
        let mut node_online = false;
        match node {
            // Only handle the offline node cause the 'already exists' node will also send 'NewProxy' event
            Some(node) if node.status == NODE_STATUS_OFFLINE => {
                let _node_status = update_node_status_more(
                    node_id,
                    subdomain,
                    device_id,
                    &version,
                    &arch,
                    &os,
                    &client_address,
                    &chrono::NaiveDateTime::from_timestamp(last_login_time, 0),
                    &now,
                    run_id,
                    NODE_STATUS_ONLINE,
                    &metas,
                )?;
                node_online = true;
            }
            None => {
                let _node_status = create_node_status(
                    node_id,
                    device_id,
                    subdomain,
                    &version,
                    &arch,
                    &os,
                    &client_address,
                    &chrono::NaiveDateTime::from_timestamp(last_login_time, 0),
                    &now,
                    run_id,
                    NODE_STATUS_ONLINE,
                    &metas,
                );
                node_online = true;
            }
            _ => {
                // Ignore if node is online
                // Ignore other Err
            }
        }
        if node_online {
            // If the node has joined some domain, add it to the redis
            if let Some(domain_node) = query_domain_node_by_node_id(node_id)? {
                let domain = domain_node.domain.as_str();
                if let Err(e) = crate::redism::nodes_upjoin(domain, node_id, domain_node.weight) {
                    log::error!(
                        "Failed to join domain nodes in redis: {}. Error msg: {}",
                        domain,
                        e
                    );
                }
            }
        }
    } else if op == "CloseProxy" {
        let content = &data["content"];
        let metas = &content["user"]["metas"];
        let device_id = match metas["deviceId"].as_str() {
            Some(id) => id,
            None => Err(format!("Device ID not found in metas: {:?}", metas))?,
        };
        if device_id.is_empty() {
            Err("Device ID is empty")?
        }
        let subdomain = content["proxy_name"].as_str().unwrap();

        // Remove the subdomain and frps_id mapping from redis
        if let Some(frps_id) = frps_id {
            if let Err(e) = crate::redism::del_subdomain(subdomain) {
                log::error!("Failed to del redis key: {}. Error msg: {}", subdomain, e);
            }
        }

        let last_active_time = chrono::Utc::now().naive_utc();

        update_node_active_status(device_id, subdomain, &last_active_time, NODE_STATUS_OFFLINE)?;
        if let Some(node) = query_node_by_subdomain(subdomain)? {
            // If the node has joined some domain, remove it to the redis
            if let Some(domain_node) = query_domain_node_by_node_id(&node.node_id)? {
                let domain = domain_node.domain.as_str();
                if let Err(e) = crate::redism::node_lefts(domain, &node.node_id, domain_node.weight)
                {
                    log::error!(
                        "Failed to leave domain nodes in redis: {}. Error msg: {}",
                        domain,
                        e
                    );
                }
            }
        }
    } else if op == "Ping" {
        let content = &data["content"];
        let metas = &content["user"]["metas"];
        let device_id = match metas["deviceId"].as_str() {
            Some(id) => id,
            None => Err(format!("Device ID not found in metas: {:?}", metas))?,
        };
        if device_id.is_empty() {
            Err("Device ID is empty")?
        }
        let last_active_time = chrono::Utc::now().naive_utc();

        // The 'Ping' event only contain device_id but without subdomain
        // Only update the online or unavail node by device_id cause there may be multiple nodes with the same device_id
        update_online_node_last_active_time(device_id, &last_active_time)?;
    }

    Ok(())
}

pub async fn query_nodes(req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    let query = req.uri().query().unwrap_or("");
    let params: HashMap<_, _> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();

    let status = params
        .get("status")
        .unwrap_or(&"".to_string())
        .parse::<String>()
        .unwrap();
    let location = params
        .get("location")
        .unwrap_or(&",,".to_string())
        .parse::<String>()
        .unwrap();
    let ids = params
        .get("ids")
        .map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<String>>());
    let device_id = params
        .get("device_id")
        .unwrap_or(&"".to_string())
        .parse::<String>()
        .unwrap();
    let chat_model = params
        .get("chat_model")
        .unwrap_or(&"".to_string())
        .parse::<String>()
        .unwrap();

    // The node must be online for at least lived_secs
    let lived_secs = params
        .get("lived_secs")
        .unwrap_or(&"0".to_string())
        .parse::<u64>()
        .unwrap_or(0);

    let mut query_parameters: HashMap<String, Value> = HashMap::new();

    if !status.is_empty() {
        query_parameters.insert("status".to_string(), Value::String(status));
    }
    if lived_secs > 0 {
        query_parameters.insert("lived_secs".to_string(), Value::Number(lived_secs.into()));
    }

    if !device_id.is_empty() {
        query_parameters.insert("device_id".to_string(), Value::String(device_id));
    }

    if !chat_model.is_empty() {
        query_parameters.insert("chat_model".to_string(), Value::String(chat_model));
    }

    if !location.is_empty() {
        let locations = location.split(",").collect::<Vec<&str>>();

        if locations.len() < 3 {
            let data = serde_json::json!({"code": 400, "msg": "Invalid location parameter"});
            let json = serde_json::to_string(&data)?;
            let response = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, "application/json")
                .body(full(json))?;
            return Ok(response);
        }

        let country = locations[0];
        let subdivision = locations[1];
        let city = locations[2];
        if !country.is_empty() {
            query_parameters.insert("country".to_string(), Value::String(country.to_string()));
        }
        if !subdivision.is_empty() {
            query_parameters.insert(
                "subdivision".to_string(),
                Value::String(subdivision.to_string()),
            );
        }
        if !city.is_empty() {
            query_parameters.insert("city".to_string(), Value::String(city.to_string()));
        }
    }

    if let Some(ids) = ids {
        if !ids.is_empty() {
            query_parameters.insert(
                "ids".to_string(),
                Value::Array(ids.into_iter().map(Value::String).collect()),
            );
        }
    }

    let result = query_nodes_by_parameters(query_parameters)?;

    let data = serde_json::json!({"code": 0, "msg": "OK", "data": result });

    let json = serde_json::to_string(&data)?;
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(full(json))?;
    Ok(response)
}

pub async fn get_living_nodes(req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    //page=0&size=10
    let query = req.uri().query().unwrap_or("");
    let params: HashMap<_, _> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();

    let page = params
        .get("page")
        .unwrap_or(&"0".to_string())
        .parse::<i64>()
        .unwrap();
    let size = params
        .get("size")
        .unwrap_or(&"10".to_string())
        .parse::<i64>()
        .unwrap();

    // The node must be online for at least lived_secs
    let lived_secs = params
        .get("lived_secs")
        .unwrap_or(&"0".to_string())
        .parse::<u64>()
        .unwrap_or(0);

    let nodes = query_living_nodes(lived_secs, page, size)?;

    let data = serde_json::json!({"code": 0, "msg": "OK", "data": nodes });

    let json = serde_json::to_string(&data)?;
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(full(json))?;
    Ok(response)
}
