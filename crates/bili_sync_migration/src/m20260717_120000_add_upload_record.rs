use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UploadRecord::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UploadRecord::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(UploadRecord::VideoId).integer())
                    .col(ColumnDef::new(UploadRecord::PageId).integer())
                    .col(ColumnDef::new(UploadRecord::YoutubeVideoId).integer())
                    .col(ColumnDef::new(UploadRecord::LocalPath).string().not_null())
                    .col(ColumnDef::new(UploadRecord::RemotePath).string().not_null())
                    .col(ColumnDef::new(UploadRecord::Status).integer().not_null().default(0))
                    .col(ColumnDef::new(UploadRecord::Attempts).integer().not_null().default(0))
                    .col(ColumnDef::new(UploadRecord::LastError).string())
                    .col(ColumnDef::new(UploadRecord::UploadedAt).timestamp())
                    .col(
                        ColumnDef::new(UploadRecord::CreatedAt)
                            .timestamp()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UploadRecord::UpdatedAt)
                            .timestamp()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .table(UploadRecord::Table)
                    .name("idx_upload_record_local_path")
                    .col(UploadRecord::LocalPath)
                    .unique()
                    .to_owned(),
            )
            .await?;
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
    Id,
    VideoId,
    PageId,
    YoutubeVideoId,
    LocalPath,
    RemotePath,
    Status,
    Attempts,
    LastError,
    UploadedAt,
    CreatedAt,
    UpdatedAt,
}
