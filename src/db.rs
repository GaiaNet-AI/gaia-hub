use diesel::connection::SimpleConnection;
use diesel::dsl::sql;
use diesel::prelude::*;
use diesel::r2d2::{self, ConnectionManager};
use diesel::sql_types::Bool;
use diesel::SqliteConnection;
use lazy_static::lazy_static;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::env;
use std::sync::Mutex;
use std::time::Duration;

use gaia_hub::*;

use crate::models;

type Pool = r2d2::Pool<ConnectionManager<SqliteConnection>>;

// To prevent error: database is locked
// https://stackoverflow.com/questions/57123453/how-to-use-diesel-with-sqlite-connections-and-avoid-database-is-locked-type-of
#[derive(Debug)]
pub struct ConnectionOptions {
    pub enable_wal: bool,
    pub enable_foreign_keys: bool,
    pub busy_timeout: Option<Duration>,
}

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error>
    for ConnectionOptions
{
    fn on_acquire(
        &self,
        conn: &mut SqliteConnection,
    ) -> std::result::Result<(), diesel::r2d2::Error> {
        (|| {
            if self.enable_wal {
                conn.batch_execute("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
            }
            if self.enable_foreign_keys {
                conn.batch_execute("PRAGMA foreign_keys = ON;")?;
            }
            if let Some(d) = self.busy_timeout {
                conn.batch_execute(&format!("PRAGMA busy_timeout = {};", d.as_millis()))?;
            }
            Ok(())
        })()
        .map_err(diesel::r2d2::Error::QueryError)
    }
}

lazy_static! {
    static ref POOL: Mutex<Pool> = {
        let database_url = env::var("DATABASE_URL").expect("No DATABASE_URL in env");
        let db_pool_size: u32 = env::var("DB_POOL_SIZE")
            .unwrap_or_else(|_| String::from("20"))
            .parse()
            .expect("DB_POOL_SIZE must be a number");
        let db_pool_min_size: u32 = env::var("DB_POOL_MIN_SIZE")
            .unwrap_or_else(|_| String::from("20"))
            .parse()
            .expect("DB_POOL_MIN_SIZE must be a number");
        let manager = ConnectionManager::<SqliteConnection>::new(database_url);
        let pool = r2d2::Pool::builder()
            .min_idle(Some(db_pool_min_size))
            .max_size(db_pool_size)
            .connection_customizer(Box::new(ConnectionOptions {
                enable_wal: true,
                enable_foreign_keys: true,
                busy_timeout: Some(Duration::from_secs(30)),
            }))
            .build(manager)
            .expect("Failed to create pool.");
        Mutex::new(pool)
    };
}

fn establish_connection() -> Result<r2d2::PooledConnection<ConnectionManager<SqliteConnection>>> {
    let pool = POOL.lock().unwrap();
    Ok(pool.get().map_err(|e| {
        log::error!("Failed to fetch db connection: {}", e);
        e
    })?)
}

pub fn create_device(
    device_id: &str,
    os: &str,
    arch: &str,
    version: &str,
    client_address: &str,
    metas: &serde_json::Value,
    login_time: &chrono::NaiveDateTime,
) -> Result<usize> {
    use crate::schema::devices;
    let _device = models::NewDevice {
        device_id,
        os,
        arch,
        version,
        client_address,
        login_time: &login_time.and_utc().timestamp(),
        meta: &metas.to_string(),
    };

    let mut conn = establish_connection()?;

    Ok(diesel::insert_into(devices::table)
        .values(&_device)
        .execute(&mut conn)?)
}

pub fn update_device(device_id: &str, login_time: &chrono::NaiveDateTime) -> Result<usize> {
    let mut conn = establish_connection()?;
    use crate::schema::devices;

    Ok(
        diesel::update(devices::table.filter(devices::device_id.eq(device_id)))
            .set((devices::login_time.eq(login_time.and_utc().timestamp()),))
            .execute(&mut conn)?,
    )
}

pub fn count_device_by_device_id(_device_id: &str) -> Result<i64> {
    let mut conn = establish_connection()?;
    use crate::schema::devices::dsl::*;
    Ok(devices
        .filter(device_id.eq(_device_id))
        .count()
        .get_result(&mut conn)?)
}

pub fn query_device_by_device_id(_device_id: &str) -> Result<Vec<models::Device>> {
    let mut conn = establish_connection()?;
    use crate::schema::devices::dsl::*;
    Ok(devices
        .filter(device_id.eq(_device_id))
        .load::<models::Device>(&mut conn)?)
}

pub fn create_node_status(
    node_id: &str,
    device_id: &str,
    subdomain: &str,
    version: &str,
    arch: &str,
    os: &str,
    client_address: &str,
    login_time: &chrono::NaiveDateTime,
    last_active_time: &chrono::NaiveDateTime,
    run_id: &str,
    status: &str,
    metas: &serde_json::Value,
) -> Result<usize> {
    let mut conn = establish_connection()?;
    use crate::schema::node_status;
    let node = models::NewNode {
        node_id,
        device_id,
        subdomain,
        version,
        arch,
        os,
        client_address,
        login_time: &login_time.and_utc().timestamp(),
        last_active_time: &last_active_time.and_utc().timestamp(),
        run_id,
        status,
        meta: &metas.to_string(),
    };

    Ok(diesel::insert_into(node_status::table)
        .values(&node)
        .execute(&mut conn)?)
}

pub fn update_node_status_more(
    node_id: &str,
    subdomain: &str,
    device_id: &str,
    version: &str,
    arch: &str,
    os: &str,
    client_address: &str,
    login_time: &chrono::NaiveDateTime,
    last_active_time: &chrono::NaiveDateTime,
    run_id: &str,
    status: &str,
    metas: &serde_json::Value,
) -> Result<usize> {
    let mut conn = establish_connection()?;
    use crate::schema::node_status;

    Ok(
        diesel::update(node_status::table.filter(node_status::node_id.eq(node_id)))
            .set((
                node_status::subdomain.eq(subdomain),
                node_status::device_id.eq(device_id),
                node_status::version.eq(version),
                node_status::arch.eq(arch),
                node_status::os.eq(os),
                node_status::client_address.eq(client_address),
                node_status::login_time.eq(login_time.and_utc().timestamp()),
                node_status::last_active_time.eq(last_active_time.and_utc().timestamp()),
                node_status::run_id.eq(run_id),
                node_status::status.eq(status),
                node_status::meta.eq(&metas.to_string()),
            ))
            .execute(&mut conn)?,
    )
}

pub fn update_online_node_last_active_time(
    device_id: &str,
    last_active_time: &chrono::NaiveDateTime,
) -> Result<usize> {
    let mut conn = establish_connection()?;
    use crate::schema::node_status;

    Ok(
        diesel::update(node_status::table.filter(node_status::device_id.eq(device_id)))
            .filter(node_status::status.eq(NODE_STATUS_ONLINE))
            .set((node_status::last_active_time.eq(last_active_time.and_utc().timestamp()),))
            .execute(&mut conn)?,
    )
}

pub fn update_node_status(
    device_id: &str,
    subdomain: &str,
    last_active_time: &chrono::NaiveDateTime,
    status: &str,
) -> Result<usize> {
    let mut conn = establish_connection()?;
    use crate::schema::node_status;

    Ok(
        diesel::update(node_status::table.filter(node_status::device_id.eq(device_id)))
            .filter(node_status::subdomain.eq(subdomain))
            .set((
                node_status::last_active_time.eq(last_active_time.and_utc().timestamp()),
                node_status::status.eq(status),
            ))
            .execute(&mut conn)?,
    )
}

pub fn update_node_avail_time_and_status(
    node_id: &str,
    last_avail_time: &chrono::NaiveDateTime,
    status: &str,
) -> Result<usize> {
    let mut conn = establish_connection()?;
    use crate::schema::node_status;

    Ok(
        diesel::update(node_status::table.filter(node_status::node_id.eq(node_id)))
            .set((
                node_status::last_avail_time.eq(last_avail_time.and_utc().timestamp()),
                node_status::status.eq(status),
            ))
            .execute(&mut conn)?,
    )
}

pub fn close_unavail_node(node_id: &str) -> Result<usize> {
    let mut conn = establish_connection()?;
    use crate::schema::node_status;

    Ok(
        diesel::update(node_status::table.filter(node_status::node_id.eq(node_id)))
            .set((node_status::status.eq(NODE_STATUS_OFFLINE),))
            .execute(&mut conn)?,
    )
}

pub fn update_node_info(
    node_id: &str,
    node_version: &str,
    chat_model: &str,
    embedding_model_name: &str,
) -> Result<usize> {
    let mut conn = establish_connection()?;
    use crate::schema::node_status;

    Ok(
        diesel::update(node_status::table.filter(node_status::node_id.eq(node_id)))
            .set((
                node_status::node_version.eq(node_version),
                node_status::chat_model.eq(chat_model),
                node_status::embedding_model.eq(embedding_model_name),
            ))
            .execute(&mut conn)?,
    )
}

pub fn query_node_by_node_id(node_id: &str) -> Result<Option<models::Node>> {
    let mut conn = establish_connection()?;
    use crate::schema::node_status::dsl::{node_id as ni, node_status};
    match node_status
        .filter(ni.eq(node_id))
        .first::<models::Node>(&mut conn)
    {
        Ok(node) => Ok(Some(node)),
        Err(diesel::NotFound) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn query_node_by_subdomain(subdomain: &str) -> Result<Option<models::Node>> {
    let mut conn = establish_connection()?;
    use crate::schema::node_status::dsl::{node_status, subdomain as sd};
    match node_status
        .filter(sd.eq(subdomain))
        .first::<models::Node>(&mut conn)
    {
        Ok(node) => Ok(Some(node)),
        Err(diesel::NotFound) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// Update node_status table, set status to offline if the last_active_time is before given time
pub fn close_expired_nodes(seconds_before: &chrono::NaiveDateTime) -> Result<usize> {
    let mut conn = establish_connection()?;
    use crate::schema::node_status;

    Ok(diesel::update(node_status::table)
        .filter(
            node_status::last_active_time
                .lt(seconds_before.and_utc().timestamp())
                .or(node_status::last_avail_time.lt(seconds_before.and_utc().timestamp())),
        )
        .filter(node_status::status.eq(NODE_STATUS_ONLINE))
        .set((node_status::status.eq(NODE_STATUS_OFFLINE),))
        .execute(&mut conn)?)
}

pub fn query_nodes_by_parameters(
    params: HashMap<String, JsonValue>,
) -> Result<Vec<models::NodeLimited>> {
    let mut conn = establish_connection()?;
    use crate::schema::node_status::dsl::*;

    let mut query = node_status.into_boxed();

    for (key, value) in params {
        match key.as_str() {
            "status" => {
                if let JsonValue::String(v) = value {
                    query = query.filter(status.eq(v));
                }
            }
            "device_id" => {
                if let JsonValue::String(v) = value {
                    query = query.filter(device_id.eq(v))
                }
            }
            "chat_model" => {
                if let JsonValue::String(v) = value {
                    query = query.filter(chat_model.eq(v))
                }
            }
            "ids" => {
                if let JsonValue::Array(v) = value {
                    let id_list: Vec<String> = v
                        .into_iter()
                        .filter_map(|val| {
                            if let JsonValue::String(s) = val {
                                Some(s)
                            } else {
                                None
                            }
                        })
                        .collect();
                    query = query.filter(node_id.eq_any(id_list));
                }
            }
            "lived_secs" => {
                if let JsonValue::Number(v) = value {
                    let lived_secs = v.as_i64().unwrap();
                    query = query.filter(sql::<Bool>(&format!(
                        "TIMESTAMPDIFF(SECOND, login_time, last_active_time) >= {}",
                        lived_secs
                    )));
                }
            }
            _ => (),
        }
    }

    Ok(query
        .select(models::NodeLimited::as_select())
        .load::<models::NodeLimited>(&mut conn)?)
}

pub fn query_living_nodes(
    lived_secs: u64,
    page: i64,
    size: i64,
) -> Result<Vec<models::LivingNode>> {
    use crate::schema::node_status::dsl::*;
    let mut conn = establish_connection()?;

    let mut query = node_status.into_boxed();

    query = query.filter(status.eq(NODE_STATUS_ONLINE));

    query = query.filter(sql::<Bool>(&format!(
        "TIMESTAMPDIFF(SECOND, login_time, last_active_time) >= {}",
        lived_secs
    )));

    Ok(query
        .order(login_time.asc())
        .limit(size)
        .offset(page * size)
        .select(models::LivingNode::as_select())
        .load::<models::LivingNode>(&mut conn)?)
}

pub fn insert_domain_node(domain: &str, node_id: &str) -> Result<usize> {
    use crate::schema::domain_nodes;
    let _domain_node = models::DomainNodes {
        domain: domain.to_string(),
        node_id: node_id.to_string(),
    };

    let mut conn = establish_connection()?;

    Ok(diesel::insert_into(domain_nodes::table)
        .values(&_domain_node)
        .execute(&mut conn)?)
}

pub fn query_domain_nodes(domain: &str) -> Result<Vec<models::DomainNodes>> {
    use crate::schema::domain_nodes::dsl::{domain as d, domain_nodes};
    let mut conn = establish_connection()?;
    let query = domain_nodes.filter(d.eq(domain));
    Ok(query.load::<models::DomainNodes>(&mut conn)?)
}

pub fn query_domain_node(domain: &str, node_id: &str) -> Result<Option<models::DomainNodes>> {
    use crate::schema::domain_nodes::dsl::{domain as d, domain_nodes, node_id as ni};
    let mut conn = establish_connection()?;
    let query = domain_nodes.filter(d.eq(domain)).filter(ni.eq(node_id));
    match query.first::<models::DomainNodes>(&mut conn) {
        Ok(node) => Ok(Some(node)),
        Err(diesel::NotFound) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn query_domain_node_by_node_id(node_id: &str) -> Result<Option<models::DomainNodes>> {
    use crate::schema::domain_nodes::dsl::{domain_nodes, node_id as ni};
    let mut conn = establish_connection()?;
    let query = domain_nodes.filter(ni.eq(node_id));
    match query.first::<models::DomainNodes>(&mut conn) {
        Ok(node) => Ok(Some(node)),
        Err(diesel::NotFound) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete_domain_node(domain: &str, node_id: &str) -> Result<usize> {
    use crate::schema::domain_nodes::dsl::{domain as d, domain_nodes, node_id as ni};
    let mut conn = establish_connection()?;
    let query = domain_nodes.filter(d.eq(domain)).filter(ni.eq(node_id));
    Ok(diesel::delete(query).execute(&mut conn)?)
}

pub fn get_distinct_domains() -> Result<Vec<String>> {
    use crate::schema::domain_nodes::dsl::{domain, domain_nodes};
    let mut conn = establish_connection()?;
    Ok(domain_nodes
        .select(domain)
        .distinct()
        .load::<String>(&mut conn)?)
}

pub fn get_nodes_by_domain(domain: &str) -> Result<Vec<String>> {
    use crate::schema::domain_nodes::dsl::{domain as d, domain_nodes, node_id};
    let mut conn = establish_connection()?;
    Ok(domain_nodes
        .filter(d.eq(domain))
        .select(node_id)
        .load::<String>(&mut conn)?)
}
