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
