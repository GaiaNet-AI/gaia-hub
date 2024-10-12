#[cfg(feature = "sqlite")]
diesel::table! {
    devices (id) {
        id -> Int4,
        device_id -> Varchar,
        version -> Varchar,
        arch -> Varchar,
        os -> Varchar,
        client_address -> Varchar,
        login_time -> Int8,
        meta -> Text,
        created_at -> Int8,
        updated_at -> Int8,
    }
}

#[cfg(feature = "mysql")]
diesel::table! {
    devices (id) {
        id -> Int8,
        device_id -> Varchar,
        version -> Varchar,
        arch -> Varchar,
        os -> Varchar,
        client_address -> Varchar,
        login_time -> Datetime,
        meta -> Json,
        created_at -> Datetime,
        updated_at -> Datetime,
    }
}

#[cfg(feature = "sqlite")]
diesel::table! {
    node_status (id) {
        id -> Int4,
        node_id -> Varchar,
        device_id -> Varchar,
        subdomain -> Varchar,
        version -> Varchar,
        arch -> Varchar,
        os -> Varchar,
        client_address -> Varchar,
        login_time -> Int8,
        last_active_time -> Int8,
        last_avail_time -> Nullable<Int8>,
        run_id -> Varchar,
        meta -> Text,
        node_version -> Varchar,
        chat_model -> Varchar,
        embedding_model -> Varchar,
        status -> Varchar,
        created_at -> Int8,
        updated_at -> Int8,
    }
}

#[cfg(feature = "mysql")]
diesel::table! {
    node_status (id) {
        id -> Int8,
        node_id -> Varchar,
        device_id -> Varchar,
        subdomain -> Varchar,
        version -> Varchar,
        arch -> Varchar,
        os -> Varchar,
        client_address -> Varchar,
        login_time -> Datetime,
        last_active_time -> Datetime,
        last_avail_time -> Nullable<Datetime>,
        run_id -> Varchar,
        meta -> Json,
        node_version -> Varchar,
        chat_model -> Varchar,
        embedding_model -> Varchar,
        status -> Varchar,
        created_at -> Datetime,
        updated_at -> Datetime,
    }
}

diesel::table! {
    domain_nodes (domain, node_id) {
        domain -> Varchar,
        node_id -> Varchar,
        weight -> Int8,
    }
}

diesel::allow_tables_to_appear_in_same_query!(domain_nodes, node_status);
