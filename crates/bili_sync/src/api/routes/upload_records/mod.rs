use axum::Router;
use axum::extract::{Extension, Query};
use axum::routing::get;
use bili_sync_entity::upload_record;
use chrono::NaiveDateTime;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};

use crate::api::wrapper::{ApiError, ApiResponse};

pub(super) fn router() -> Router {
    Router::new().route("/upload-records", get(get_upload_records))
}

#[derive(Deserialize)]
pub struct UploadRecordsRequest {
    video_id: Option<i32>,
    page_id: Option<i32>,
    youtube_video_id: Option<i32>,
    status: Option<i32>,
    #[serde(default = "default_page")]
    page: u64,
    #[serde(default = "default_page_size")]
    page_size: u64,
}

fn default_page() -> u64 {
    0
}

fn default_page_size() -> u64 {
    50
}

#[derive(Serialize)]
pub struct UploadRecordResponse {
    id: i32,
    video_id: Option<i32>,
    page_id: Option<i32>,
    youtube_video_id: Option<i32>,
    local_path: String,
    remote_path: String,
    status: i32,
    attempts: i32,
    last_error: Option<String>,
    uploaded_at: Option<NaiveDateTime>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

pub async fn get_upload_records(
    Extension(db): Extension<DatabaseConnection>,
    Query(params): Query<UploadRecordsRequest>,
) -> Result<ApiResponse<Vec<UploadRecordResponse>>, ApiError> {
    let mut query = upload_record::Entity::find();
    if let Some(video_id) = params.video_id {
        query = query.filter(upload_record::Column::VideoId.eq(video_id));
    }
    if let Some(page_id) = params.page_id {
        query = query.filter(upload_record::Column::PageId.eq(page_id));
    }
    if let Some(youtube_video_id) = params.youtube_video_id {
        query = query.filter(upload_record::Column::YoutubeVideoId.eq(youtube_video_id));
    }
    if let Some(status) = params.status {
        query = query.filter(upload_record::Column::Status.eq(status));
    }
    let records = query
        .order_by_desc(upload_record::Column::UpdatedAt)
        .paginate(&db, params.page_size)
        .fetch_page(params.page)
        .await?
        .into_iter()
        .map(|record| UploadRecordResponse {
            id: record.id,
            video_id: record.video_id,
            page_id: record.page_id,
            youtube_video_id: record.youtube_video_id,
            local_path: record.local_path,
            remote_path: record.remote_path,
            status: record.status,
            attempts: record.attempts,
            last_error: record.last_error,
            uploaded_at: record.uploaded_at,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
        .collect();
    Ok(ApiResponse::ok(records))
}
