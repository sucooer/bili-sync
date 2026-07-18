//! `SeaORM` Entity for auto-upload records.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Default)]
#[sea_orm(table_name = "upload_record")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub video_id: Option<i32>,
    pub page_id: Option<i32>,
    pub youtube_video_id: Option<i32>,
    pub local_path: String,
    pub remote_path: String,
    pub status: i32,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub uploaded_at: Option<DateTime>,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
