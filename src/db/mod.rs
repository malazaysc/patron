use crate::app::RuntimePaths;

pub struct StateStoreStatus<'a> {
    pub engine: &'a str,
    pub location: String,
}

pub fn state_store_status(runtime: &RuntimePaths) -> StateStoreStatus<'_> {
    StateStoreStatus {
        engine: "sqlite",
        location: runtime
            .relative_to_repo(&runtime.state_db)
            .display()
            .to_string(),
    }
}
