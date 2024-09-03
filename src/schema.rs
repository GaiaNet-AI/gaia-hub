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
