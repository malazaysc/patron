pub mod schema;

use crate::app::RuntimePaths;

pub struct StateStoreStatus<'a> {
    pub engine: &'a str,
    pub initial_schema_bytes: usize,
    pub location: String,
    pub schema_version: i64,
}

pub fn state_store_status(runtime: &RuntimePaths) -> StateStoreStatus<'_> {
    StateStoreStatus {
        engine: "sqlite",
        initial_schema_bytes: initial_schema_sql().len(),
        location: runtime
            .relative_to_repo(&runtime.state_db)
            .display()
            .to_string(),
        schema_version: schema::CURRENT_SCHEMA_VERSION,
    }
}

pub fn initial_schema_sql() -> &'static str {
    schema::INITIAL_SCHEMA
}
