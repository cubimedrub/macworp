use macworp_core::entities::LogStream;
use sea_orm::entity::prelude::*;

/// A persisted chunk of consecutive log lines of one stream, deleted together with its run.
///
/// Lines are stored in batches (the worker flushes by time or size) instead of one row per line,
/// since a run can produce thousands of lines.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "run_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Run this line belongs to
    pub run_id: i64,
    /// Position of the chunk within the run, unique per run and increasing; used as resume cursor for streaming (`seq > last_seen`)
    pub seq: i64,
    /// Where the lines came from
    pub stream: LogStream,
    /// When the chunk was recorded
    pub created_at: DateTimeUtc,
    /// The lines joined by `\n`, without trailing newline
    pub content: String,
    /// Number of lines in `content`
    pub line_count: i32,

    #[sea_orm(belongs_to, from = "run_id", to = "id")]
    pub run: HasOne<super::run::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
