pub const INITIAL_SCHEMA: &str = include_str!("migrations/0001_initial.sql");
pub const RUNTIME_METADATA_SCHEMA: &str = include_str!("migrations/0002_runtime_metadata.sql");
pub const INTAKE_AND_ACTIVITY_SCHEMA: &str =
    include_str!("migrations/0003_intake_and_activity.sql");

pub const CURRENT_SCHEMA_VERSION: i64 = 3;

pub const MIGRATIONS: &[(i64, &str)] = &[
    (1, INITIAL_SCHEMA),
    (2, RUNTIME_METADATA_SCHEMA),
    (3, INTAKE_AND_ACTIVITY_SCHEMA),
];
