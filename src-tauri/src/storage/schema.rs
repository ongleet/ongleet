// The existing SQLite schema uses 64-bit integers, including INTEGER primary keys.
diesel::table! {
    projects (id) {
        id -> BigInt,
        name -> Text,
        root_directory -> Text,
        display_order -> BigInt,
        collapsed -> Bool,
    }
}

diesel::table! {
    sessions (id) {
        id -> BigInt,
        name -> Text,
        base_directory -> Text,
        display_order -> BigInt,
        selected_tab_id -> Nullable<BigInt>,
        project_id -> BigInt,
        pull_request_json -> Nullable<Text>,
        plugin_data_json -> Text,
    }
}

diesel::table! {
    tab_definitions (id) {
        id -> BigInt,
        session_id -> BigInt,
        name -> Text,
        kind -> Text,
        config_json -> Text,
        display_order -> BigInt,
        plugin_data_json -> Text,
    }
}

diesel::table! {
    open_tabs (definition_id) {
        definition_id -> BigInt,
        resume_on_start -> Bool,
    }
}

diesel::table! {
    app_state (singleton) {
        singleton -> BigInt,
        selected_session_id -> Nullable<BigInt>,
        session_sequence -> BigInt,
        tab_sequence -> BigInt,
        project_sequence -> BigInt,
    }
}

diesel::table! {
    archives (id) {
        id -> BigInt,
        payload_json -> Text,
    }
}

diesel::table! {
    archive_state (singleton) {
        singleton -> BigInt,
        sequence -> BigInt,
    }
}

diesel::joinable!(tab_definitions -> sessions (session_id));
diesel::joinable!(open_tabs -> tab_definitions (definition_id));
diesel::allow_tables_to_appear_in_same_query!(
    projects,
    sessions,
    tab_definitions,
    open_tabs,
    app_state,
    archives,
    archive_state
);
