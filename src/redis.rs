use redis::{Commands, Connection};
use std::env;

use gaia_hub::*;

pub fn establish_redis_conn() -> Result<Connection> {
    let redis_url = env::var("REDIS_URL")?;
    let client = redis::Client::open(redis_url)?;
    let con = client.get_connection()?;
    Ok(con)
}

pub fn set_subdomain_frps_id(subdomain: &str, frps_id: &str) -> Result<()> {
    let mut conn = establish_redis_conn()?;
    conn.set::<&str, &str, String>(subdomain, frps_id)?;
    Ok(())
}

pub fn del_subdomain(subdomain: &str) -> Result<()> {
    let mut conn = establish_redis_conn()?;
    conn.del::<&str, i32>(subdomain)?;
    Ok(())
}

pub fn nodes_join(domain: &str, node_ids: Vec<&str>) -> Result<()> {
    let mut conn = establish_redis_conn()?;
    conn.lpush::<&str, Vec<&str>, i32>(domain, node_ids)?;
    Ok(())
}

pub fn node_lefts(domain: &str, node_id: &str) -> Result<()> {
    let mut conn = establish_redis_conn()?;
    // Pass 0 as the count to remove all elements equal to subdomain.
    conn.lrem::<&str, &str, i32>(domain, 0, node_id)?;
    Ok(())
}

pub fn get_domain_nodes(domain: &str) -> Result<Vec<String>> {
    let mut conn = establish_redis_conn()?;
    let nodes: Vec<String> = conn.lrange(domain, 0, -1)?;
    Ok(nodes)
}
