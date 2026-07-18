use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("PRAGMA foreign_keys = OFF").await?;
        db.execute_unprepared("ALTER TABLE upload_record RENAME TO upload_record_old")
            .await?;
        db.execute_unprepared(
            "CREATE TABLE upload_record (
                id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                video_id INTEGER,
                page_id INTEGER,
                youtube_video_id INTEGER,
                local_path VARCHAR NOT NULL,
                remote_path VARCHAR NOT NULL,
                status INTEGER NOT NULL DEFAULT 0,
                attempts INTEGER NOT NULL DEFAULT 0,
                last_error VARCHAR,
                uploaded_at TIMESTAMP,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL
            )",
        )
        .await?;
        db.execute_unprepared("INSERT INTO upload_record SELECT * FROM upload_record_old")
            .await?;
        db.execute_unprepared("DROP TABLE upload_record_old").await?;
        db.execute_unprepared("CREATE UNIQUE INDEX idx_upload_record_local_path ON upload_record (local_path)")
            .await?;
        db.execute_unprepared("PRAGMA foreign_keys = ON").await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UploadRecord::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum UploadRecord {
    Table,
}
