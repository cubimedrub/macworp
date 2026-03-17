use sea_orm_migration::prelude::*;

#[tokio::main]
async fn main() {
    cli::run_cli(macworp_database_migration::Migrator).await;
}
