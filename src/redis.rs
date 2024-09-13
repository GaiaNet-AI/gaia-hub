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

fn compose_key_name(domain: &str) -> String {
    format!("{}_nodes_weights", domain)
}

pub fn nodes_join(domain: &str, node_id: &str, weight: i64) -> Result<()> {
    let mut conn = establish_redis_conn()?;
    let key = compose_key_name(domain);
    let key = key.as_str();
    redis::transaction(&mut conn, &[key], |con, pipe| {
        let last_member: Vec<(String, i64)> = con.zrange_withscores(key, -1, -1)?;
        if last_member.len() == 0 {
            pipe.zadd(key, node_id, weight).ignore().query(con)
        } else {
            let last_member = last_member.get(0).unwrap();
            let last_score = last_member.1;
            pipe.zadd(key, node_id, weight + last_score)
                .ignore()
                .query(con)
        }
    })?;
    Ok(())
}

pub fn nodes_upjoin(domain: &str, node_id: &str, weight: i64) -> Result<()> {
    let mut conn = establish_redis_conn()?;
    let key = compose_key_name(domain);
    let key = key.as_str();
    redis::transaction(&mut conn, &[key], |con, pipe| {
        match con.zrank(key, node_id)? {
            Some(rank) => {
                let (old_weight, following_members): (i64, Vec<(String, i64)>) = match rank {
                    0 => {
                        // If the node is the first member, the old weight is the first member's weight
                        let following_members: Vec<(String, i64)> =
                            con.zrange_withscores(key, rank, -1)?;
                        let first_score = following_members.get(0).unwrap().1;
                        (first_score, following_members)
                    }
                    _ => {
                        let mut following_members: Vec<(String, i64)> =
                            con.zrange_withscores(key, rank - 1, -1)?;
                        let first_score = following_members.get(0).unwrap().1;
                        let second_score = following_members.get(1).unwrap().1;
                        following_members.remove(0);
                        (second_score - first_score, following_members)
                    }
                };

                let changed_weight = weight - old_weight;
                if changed_weight == 0 {
                    return pipe.query(con);
                }
                for member in following_members {
                    pipe.zadd(key, member.0, member.1 + changed_weight).ignore();
                }
                pipe.query(con)
            }
            None => {
                let last_member: Vec<(String, i64)> = con.zrange_withscores(key, -1, -1)?;
                if last_member.len() == 0 {
                    pipe.zadd(key, node_id, weight).ignore().query(con)
                } else {
                    let last_member = last_member.get(0).unwrap();
                    let last_score = last_member.1;
                    pipe.zadd(key, node_id, weight + last_score)
                        .ignore()
                        .query(con)
                }
            }
        }
    })?;
    Ok(())
}

pub fn node_lefts(domain: &str, node_id: &str, weight: i64) -> Result<()> {
    let mut conn = establish_redis_conn()?;
    let key = compose_key_name(domain);
    let key = key.as_str();
    redis::transaction(&mut conn, &[key], |con, pipe| {
        let rank: Option<isize> = con.zrank(key, node_id)?;
        if rank.is_none() {
            return pipe.query(con);
        }
        let rank = rank.unwrap();
        let following_members: Vec<(String, i64)> = con.zrange_withscores(key, rank + 1, -1)?;
        for member in following_members {
            pipe.zadd(key, member.0, member.1 - weight).ignore();
        }
        pipe.zrem(key, node_id).ignore().query(con)
    })?;
    Ok(())
}

pub fn get_domain_nodes(domain: &str) -> Result<Vec<(String, i64)>> {
    let mut conn = establish_redis_conn()?;
    let key = compose_key_name(domain);
    let key = key.as_str();
    let mut nodes: Vec<(String, i64)> = conn.zrange_withscores(key, 0, -1)?;
    for i in 0..nodes.len() {
        let l = nodes.len() - i - 1;
        if i < nodes.len() - 1 {
            nodes[l].1 = nodes[l].1 - nodes[l - 1].1;
        }
    }
    Ok(nodes)
}
