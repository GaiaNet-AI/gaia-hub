use bytes::Buf;
use http_body_util::BodyExt;
use hyper::{body::Incoming as IncomingBody, header, Request, Response, StatusCode};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;

use crate::db::*;
use gaia_hub::*;

lazy_static! {
    static ref DOMAIN_NAME_RE: Regex = Regex::new(r"^[\w\-]+$").unwrap();
}

#[derive(Debug, serde::Deserialize)]
struct NodeWeight {
    node_id: String,
    weight: i64,
}

#[derive(Debug, serde::Deserialize)]
struct DomainNodesWeights {
    domain: String,
    nodes_weights: Vec<NodeWeight>,
}

#[derive(Debug, serde::Deserialize)]
struct DomainNodes {
    domain: String,
    nodes_ids: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum CreateResultCode {
    Created,
    NodeNotExist,
    NodeOffline,
}

#[derive(Debug, serde::Serialize)]
struct CreateResult {
    domain: String,
    node_id: String,
    code: CreateResultCode,
}

pub async fn create_domain_node(req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    let whole_body = req.collect().await?.aggregate();
    let domain_nodes: Vec<DomainNodesWeights> = match serde_json::from_reader(whole_body.reader()) {
        Ok(data) => data,
        Err(e) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(full(format!("Invalid JSON: {}", e)))?)
        }
    };

    let mut results = vec![];

    for domain_node in domain_nodes {
        let domain = domain_node.domain;
        if !DOMAIN_NAME_RE.is_match(&domain) {
            continue;
        }
        // domain must be lowercase
        let domain = domain.to_lowercase();

        let nodes_weights = domain_node.nodes_weights;

        for node_weight in nodes_weights {
            let r = CreateResult {
                domain: domain.clone(),
                node_id: node_weight.node_id.clone(),
                code: CreateResultCode::Created,
            };
            results.push(r);
            let l = results.len();
            let r = results.get_mut(l - 1).unwrap();

            let domain_node = query_domain_node(&domain, &node_weight.node_id)?;

            if domain_node.is_some() {
                if domain_node.unwrap().weight != node_weight.weight {
                    let updated =
                        update_domain_node(&domain, &node_weight.node_id, node_weight.weight)?;
                    if updated > 0 {
                        crate::redism::nodes_upjoin(
                            &domain,
                            &node_weight.node_id,
                            node_weight.weight,
                        )?;
                    }
                }
                continue;
            }

            let node = query_node_by_node_id(&node_weight.node_id)?;

            // Only online nodes can be added to domain
            if node.is_none() {
                r.code = CreateResultCode::NodeNotExist;
                continue;
            }
            if node.unwrap().status != NODE_STATUS_ONLINE {
                r.code = CreateResultCode::NodeOffline;
                continue;
            }

            let inserted = insert_domain_node(&domain, &node_weight.node_id, node_weight.weight)?;
            if inserted > 0 {
                crate::redism::nodes_join(&domain, &node_weight.node_id, node_weight.weight)?;
            }
        }
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(full(serde_json::to_string(&results).unwrap()))?)
}

pub async fn get_domain_nodes(req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    let query = req.uri().query().unwrap_or("");
    let params: HashMap<_, _> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();

    let domain = match params.get("domain") {
        Some(domain) => domain.to_lowercase(),
        None => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(full(String::from("domain is required")))?)
        }
    };

    let nodes = query_domain_nodes(&domain)?;

    let data = serde_json::json!({"code": 0, "msg": "OK", "data": nodes });

    let json = serde_json::to_string(&data)?;
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(full(json))?;
    Ok(response)
}

pub async fn remove_domain_node(req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    let whole_body = req.collect().await?.aggregate();
    let domain_nodes: Vec<DomainNodes> = match serde_json::from_reader(whole_body.reader()) {
        Ok(data) => data,
        Err(e) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(full(format!("Invalid JSON: {}", e)))?)
        }
    };

    for domain_node in domain_nodes {
        let domain = domain_node.domain;
        if !DOMAIN_NAME_RE.is_match(&domain) {
            continue;
        }
        // domain must be lowercase
        let domain = domain.to_lowercase();

        let nodes_ids = domain_node.nodes_ids;

        for node_id in nodes_ids {
            if let Some(deleted) = delete_domain_node(&domain, &node_id)? {
                crate::redism::node_lefts(&domain, &node_id, deleted.weight)?;
            }
        }
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(full(String::from("Domain node deleted")))?)
}
