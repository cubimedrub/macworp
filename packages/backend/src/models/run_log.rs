use macworp_core::entities::LogStream;
use sea_orm::{ConnectionTrait, QuerySelect, Set, entity::prelude::*};

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

/// Content of the single chunk written when a run exceeds its log limit
pub const TRUNCATION_MARKER: &str = "[log truncated: size limit reached, further output dropped]";

impl Entity {
    /// Appends one chunk of `lines` to the logs of a run as the next `seq`.
    ///
    /// `max_bytes` limits the stored log per run (0 = unlimited). The chunk that crosses the limit
    /// is still stored; after that one [`TRUNCATION_MARKER`] chunk is written and everything further is dropped.
    pub async fn append<C: ConnectionTrait>(
        db: &C,
        run_id: i64,
        stream: LogStream,
        lines: &[String],
        max_bytes: u64,
    ) -> Result<(), DbErr> {
        if lines.is_empty() {
            return Ok(());
        }
        let (next_seq, total_bytes) = Self::position(db, run_id).await?;
        let (stream, content, line_count) = if max_bytes != 0 && total_bytes >= max_bytes {
            let marked = Entity::find()
                .filter(Column::RunId.eq(run_id))
                .filter(Column::Content.eq(TRUNCATION_MARKER))
                .count(db)
                .await?
                > 0;
            if marked {
                return Ok(());
            }
            (LogStream::Engine, TRUNCATION_MARKER.to_string(), 1)
        } else {
            (stream, lines.join("\n"), lines.len() as i32)
        };
        let mut seq = next_seq;
        let mut attempts = 0;
        loop {
            let result = ActiveModel {
                run_id: Set(run_id),
                seq: Set(seq),
                stream: Set(stream),
                created_at: Set(chrono::Utc::now()),
                content: Set(content.clone()),
                line_count: Set(line_count),
                ..Default::default()
            }
            .insert(db)
            .await;
            match result {
                Ok(_) => return Ok(()),
                // another writer took this seq (unique index on run_id, seq)
                Err(_) if attempts < 3 => {
                    attempts += 1;
                    seq = Self::position(db, run_id).await?.0;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Next free `seq` and stored content size in bytes of a run
    async fn position<C: ConnectionTrait>(db: &C, run_id: i64) -> Result<(i64, u64), DbErr> {
        let row: Option<(i64, i64)> = Entity::find()
            .filter(Column::RunId.eq(run_id))
            .select_only()
            .column_as(
                Expr::cust("CAST(COALESCE(MAX(seq) + 1, 0) AS BIGINT)"),
                "next_seq",
            )
            .column_as(
                Expr::cust("CAST(COALESCE(SUM(LENGTH(content)), 0) AS BIGINT)"),
                "total",
            )
            .into_tuple()
            .one(db)
            .await?;
        let (next_seq, total) = row.unwrap_or((0, 0));
        Ok((next_seq, total.max(0) as u64))
    }
}
