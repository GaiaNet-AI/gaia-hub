use crate::schema::*;
use diesel::prelude::*;

use serde::Serialize;

#[derive(Serialize, Queryable, Selectable)]
#[diesel(table_name = crate::schema::node_status)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Node {
    pub id: i32,
    pub node_id: String,
    pub device_id: String,
    pub subdomain: String,
    pub version: String,
    pub arch: String,
    pub os: String,
    pub client_address: String,
    pub login_time: i64,
    pub last_active_time: i64,
    pub last_avail_time: Option<i64>,
    pub run_id: String,
    pub meta: String,
    pub node_version: String,
    pub chat_model: String,
    pub embedding_model: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Queryable, Selectable)]
#[diesel(table_name = crate::schema::node_status)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct NodeLimited {
    pub subdomain: String,
    pub node_id: String,
    pub status: String,
    pub node_version: String,
    pub chat_model: String,
    pub embedding_model: String,
    pub device_id: String,
    pub client_address: String,
}

#[derive(Serialize, Selectable, Queryable)]
#[diesel(table_name = node_status)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct LivingNode {
    pub node_id: String,
    pub subdomain: String,
}

#[derive(Serialize, Insertable, AsChangeset)]
#[diesel(table_name = node_status)]
pub struct NewNode<'a> {
    pub node_id: &'a str,
    pub device_id: &'a str,
    pub subdomain: &'a str,
    pub version: &'a str,
    pub arch: &'a str,
    pub os: &'a str,
    pub client_address: &'a str,
    pub status: &'a str,
    pub login_time: &'a i64,
    pub last_active_time: &'a i64,
    pub run_id: &'a str,
    pub meta: &'a str,
}

#[derive(Serialize, Insertable, AsChangeset)]
#[diesel(table_name = devices)]
pub struct NewDevice<'a> {
    pub device_id: &'a str,
    pub version: &'a str,
    pub arch: &'a str,
    pub os: &'a str,
    pub client_address: &'a str,
    pub login_time: &'a i64,
    pub meta: &'a str,
}

#[derive(Serialize, Queryable, Selectable)]
#[diesel(table_name = crate::schema::devices)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Device {
    pub id: i32,
    pub device_id: String,
    pub version: String,
    pub arch: String,
    pub os: String,
    pub client_address: String,
    pub login_time: i64,
    pub meta: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Insertable, AsChangeset, Queryable, Selectable)]
#[diesel(table_name = crate::schema::domain_nodes)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DomainNodes {
    pub domain: String,
    pub node_id: String,
    pub weight: i64,
}
