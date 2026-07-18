use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use bili_sync_entity::upload_record;
use chrono::Utc;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use sea_orm::ActiveValue::Set;
use sea_orm::entity::prelude::*;
use serde::Deserialize;
use tokio::fs;
use tokio_util::io::ReaderStream;

use crate::config::{AutoUploadOption, OpenListAuth};

const STATUS_PENDING: i32 = 0;
const STATUS_UPLOADING: i32 = 1;
const STATUS_SUCCEEDED: i32 = 2;
const STATUS_FAILED: i32 = 3;

/// 上传 B 站分页视频所在目录下的所有文件，保留下载创建的文件夹结构。
///
/// `strip_base` 为下载根目录（视频源路径），远程路径会保留 `strip_base` 之后的完整相对路径，
/// 包括视频目录名及其内部子目录（如 `Season 1/`）。
/// `local_path` 为分页视频文件路径，函数会自动取其父目录，并仅上传其中与该视频文件名
/// 前缀（去除扩展名）匹配的文件，避免多页并发时重复上传其他分页的文件。
pub async fn upload_downloaded_file(
    connection: &DatabaseConnection,
    config: &AutoUploadOption,
    video_id: i32,
    page_id: i32,
    strip_base: &Path,
    local_path: &Path,
) -> Result<()> {
    let Some(local_dir) = local_path.parent() else {
        bail!("无法获取视频文件的父目录：{}", local_path.display());
    };
    let name_prefix = file_stem(local_path);
    upload_directory(
        connection,
        config,
        Some(video_id),
        Some(page_id),
        None,
        strip_base,
        local_dir,
        Some(&name_prefix),
    )
    .await
}

/// 上传 YouTube 视频所在目录下的所有文件，保留下载创建的文件夹结构。
///
/// `strip_base` 为频道目录路径，远程路径会保留 `strip_base` 之后的完整相对路径。
/// `local_path` 为视频文件路径，函数会自动取其父目录并上传其中所有文件。
pub async fn upload_youtube_file(
    connection: &DatabaseConnection,
    config: &AutoUploadOption,
    youtube_video_id: i32,
    strip_base: &Path,
    local_path: &Path,
) -> Result<()> {
    let Some(local_dir) = local_path.parent() else {
        bail!("无法获取 YouTube 视频文件的父目录：{}", local_path.display());
    };
    upload_directory(
        connection,
        config,
        None,
        None,
        Some(youtube_video_id),
        strip_base,
        local_dir,
        None,
    )
    .await
}

/// 上传 `local_dir` 目录下的所有文件到 OpenList，保留 `strip_base` 之后的完整目录结构。
/// 当 `name_prefix` 为 `Some` 时，仅上传文件名以该前缀开头的文件。
#[allow(clippy::too_many_arguments)]
async fn upload_directory(
    connection: &DatabaseConnection,
    config: &AutoUploadOption,
    video_id: Option<i32>,
    page_id: Option<i32>,
    youtube_video_id: Option<i32>,
    strip_base: &Path,
    local_dir: &Path,
    name_prefix: Option<&str>,
) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }
    if !local_dir.is_dir() {
        bail!("待上传目录不存在或不是目录：{}", local_dir.display());
    }
    let files = collect_files(local_dir, name_prefix).await?;
    if files.is_empty() {
        bail!("待上传目录为空：{}", local_dir.display());
    }
    let client = OpenListClient::new(config).await?;
    let mut errors = Vec::new();
    for file_path in &files {
        match upload_single_file(
            connection,
            config,
            &client,
            video_id,
            page_id,
            youtube_video_id,
            strip_base,
            file_path,
        )
        .await
        {
            Ok(()) => {}
            Err(e) => {
                let message = format!("上传文件 {} 失败：{:#}", file_path.display(), e);
                error!("{}", message);
                errors.push(message);
            }
        }
    }
    if config.delete_local_after_upload && errors.is_empty() {
        for file_path in &files {
            fs::remove_file(file_path)
                .await
                .with_context(|| format!("删除本地文件失败：{}", file_path.display()))?;
        }
        remove_empty_dirs_up_to(local_dir, strip_base).await?;
    }
    if errors.is_empty() {
        Ok(())
    } else {
        bail!(errors.join("\n"))
    }
}

/// 从 `leaf_dir` 向上递归删除空目录，直到 `stop_base` 或遇到非空目录。
async fn remove_empty_dirs_up_to(leaf_dir: &Path, stop_base: &Path) -> Result<()> {
    let canonical_stop = dunce::canonicalize(stop_base)?;
    let mut dir = dunce::canonicalize(leaf_dir)?;
    while dir.starts_with(&canonical_stop) && dir != canonical_stop {
        let mut entries = fs::read_dir(&dir).await?;
        if entries.next_entry().await?.is_some() {
            break;
        }
        fs::remove_dir(&dir)
            .await
            .with_context(|| format!("删除空目录失败：{}", dir.display()))?;
        dir = dir
            .parent()
            .context("unreachable: stop_base ensured non-empty parent")?
            .to_path_buf();
    }
    Ok(())
}

/// 收集目录下的所有文件（递归），返回绝对路径列表。最大深度 100 防止符号链接循环。
/// 当 `name_prefix` 为 `Some` 时，仅收集文件名以该前缀开头的文件。
async fn collect_files(dir: &Path, name_prefix: Option<&str>) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![(PathBuf::from(dir), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        if depth > 100 {
            warn!("目录嵌套过深（>100），跳过：{}", current.display());
            continue;
        }
        let mut entries = fs::read_dir(&current)
            .await
            .with_context(|| format!("读取目录失败：{}", current.display()))?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                stack.push((path, depth + 1));
            } else if file_type.is_file() {
                if let Some(prefix) = name_prefix {
                    let matches = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .is_some_and(|stem| stem == prefix || stem.starts_with(prefix));
                    if !matches {
                        continue;
                    }
                }
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// 获取路径对应文件的文件名（不含扩展名）。
fn file_stem(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
}

/// 上传单个文件，带重试。文件相对 `strip_base` 的路径用于生成远程路径。
#[allow(clippy::too_many_arguments)]
async fn upload_single_file(
    connection: &DatabaseConnection,
    config: &AutoUploadOption,
    client: &OpenListClient,
    video_id: Option<i32>,
    page_id: Option<i32>,
    youtube_video_id: Option<i32>,
    strip_base: &Path,
    local_path: &Path,
) -> Result<()> {
    let relative = local_path
        .strip_prefix(strip_base)
        .with_context(|| format!("无法计算相对路径：{}", local_path.display()))?;
    let remote_path = remote_path(config, relative)?;
    let local_path_str = local_path.to_string_lossy().to_string();
    let existing = upload_record::Entity::find()
        .filter(upload_record::Column::LocalPath.eq(&local_path_str))
        .one(connection)
        .await?;
    if existing
        .as_ref()
        .is_some_and(|record| record.status == STATUS_SUCCEEDED)
    {
        return Ok(());
    }
    let record = upsert_upload_record(
        connection,
        existing,
        video_id,
        page_id,
        youtube_video_id,
        &local_path_str,
        &remote_path,
    )
    .await?;
    let mut last_error = None;
    for attempt in record.attempts + 1..=config.retry_attempts as i32 {
        mark_uploading(connection, record.id, attempt).await?;
        match client.upload(local_path, &remote_path).await {
            Ok(()) => {
                mark_succeeded(connection, record.id).await?;
                return Ok(());
            }
            Err(e) => {
                let message = format!("{:#}", e);
                last_error = Some(message.clone());
                mark_failed(connection, record.id, &message).await?;
                if attempt < config.retry_attempts as i32 {
                    tokio::time::sleep(Duration::from_secs(config.retry_delay_secs)).await;
                }
            }
        }
    }
    bail!(last_error.unwrap_or_else(|| "自动上传失败".to_string()))
}

async fn upsert_upload_record(
    connection: &DatabaseConnection,
    existing: Option<upload_record::Model>,
    video_id: Option<i32>,
    page_id: Option<i32>,
    youtube_video_id: Option<i32>,
    local_path: &str,
    remote_path: &str,
) -> Result<upload_record::Model> {
    if let Some(record) = existing {
        return Ok(record);
    }
    let now = Utc::now().naive_utc();
    let record = upload_record::ActiveModel {
        video_id: Set(video_id),
        page_id: Set(page_id),
        youtube_video_id: Set(youtube_video_id),
        local_path: Set(local_path.to_string()),
        remote_path: Set(remote_path.to_string()),
        status: Set(STATUS_PENDING),
        attempts: Set(0),
        last_error: Set(None),
        uploaded_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    Ok(record.insert(connection).await?)
}

async fn mark_uploading(connection: &DatabaseConnection, id: i32, attempt: i32) -> Result<()> {
    upload_record::ActiveModel {
        id: Set(id),
        status: Set(STATUS_UPLOADING),
        attempts: Set(attempt),
        updated_at: Set(Utc::now().naive_utc()),
        ..Default::default()
    }
    .update(connection)
    .await?;
    Ok(())
}

async fn mark_succeeded(connection: &DatabaseConnection, id: i32) -> Result<()> {
    let now = Utc::now().naive_utc();
    upload_record::ActiveModel {
        id: Set(id),
        status: Set(STATUS_SUCCEEDED),
        last_error: Set(None),
        uploaded_at: Set(Some(now)),
        updated_at: Set(now),
        ..Default::default()
    }
    .update(connection)
    .await?;
    Ok(())
}

async fn mark_failed(connection: &DatabaseConnection, id: i32, error: &str) -> Result<()> {
    upload_record::ActiveModel {
        id: Set(id),
        status: Set(STATUS_FAILED),
        last_error: Set(Some(error.to_string())),
        updated_at: Set(Utc::now().naive_utc()),
        ..Default::default()
    }
    .update(connection)
    .await?;
    Ok(())
}

/// 根据文件相对于下载目录的相对路径，拼接到 OpenList 的 remote_dir 下。
fn remote_path(config: &AutoUploadOption, relative: &Path) -> Result<String> {
    let dir = config.openlist.remote_dir.trim_end_matches('/');
    let relative_str = relative.to_str().context("文件路径包含非 UTF-8 字符，无法上传")?;
    Ok(format!("{}/{}", dir, relative_str))
}

struct OpenListClient {
    endpoint: String,
    token: String,
    client: reqwest::Client,
    created_dirs: tokio::sync::Mutex<HashSet<String>>,
}

impl OpenListClient {
    async fn new(config: &AutoUploadOption) -> Result<Self> {
        let endpoint = config.openlist.endpoint.trim_end_matches('/').to_string();
        if endpoint.is_empty() {
            bail!("OpenList 地址为空");
        }
        let client = reqwest::Client::new();
        let token = match &config.openlist.auth {
            OpenListAuth::Token { token } => token.clone(),
            OpenListAuth::Password { username, password } => login(&client, &endpoint, username, password).await?,
            OpenListAuth::None => bail!("OpenList 自动上传未配置认证信息"),
        };
        Ok(Self {
            endpoint,
            token,
            client,
            created_dirs: tokio::sync::Mutex::new(HashSet::new()),
        })
    }

    async fn upload(&self, local_path: &Path, remote_path: &str) -> Result<()> {
        self.ensure_parent_dir(remote_path).await?;
        let file = fs::File::open(local_path)
            .await
            .with_context(|| format!("打开待上传文件失败：{}", local_path.display()))?;
        let stream = ReaderStream::new(file);
        let body = reqwest::Body::wrap_stream(stream);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&self.token)?);
        headers.insert("File-Path", HeaderValue::from_str(&percent_encode(remote_path))?);
        self.client
            .put(format!("{}/api/fs/put", self.endpoint))
            .headers(headers)
            .body(body)
            .send()
            .await?
            .check_openlist_response()
            .await?;
        Ok(())
    }

    async fn ensure_parent_dir(&self, remote_path: &str) -> Result<()> {
        let Some((dir, _)) = remote_path.rsplit_once('/') else {
            return Ok(());
        };
        if dir.is_empty() {
            return Ok(());
        }
        let mut created = self.created_dirs.lock().await;
        if created.contains(dir) {
            return Ok(());
        }
        self.client
            .post(format!("{}/api/fs/mkdir", self.endpoint))
            .header(AUTHORIZATION, &self.token)
            .json(&serde_json::json!({ "path": dir }))
            .send()
            .await?
            .check_openlist_response()
            .await?;
        created.insert(dir.to_string());
        Ok(())
    }
}

async fn login(client: &reqwest::Client, endpoint: &str, username: &str, password: &str) -> Result<String> {
    let response = client
        .post(format!("{}/api/auth/login", endpoint))
        .json(&serde_json::json!({ "username": username, "password": password }))
        .send()
        .await?
        .error_for_status()?;
    let value: OpenListResponse<LoginData> = response.json().await?;
    ensure_openlist_success(value.code, &value.message)?;
    value
        .data
        .map(|data| data.token)
        .ok_or_else(|| anyhow!("OpenList 登录响应中缺少 token"))
}

#[derive(Deserialize)]
struct OpenListResponse<T> {
    code: i32,
    message: String,
    data: Option<T>,
}

#[derive(Deserialize)]
struct LoginData {
    token: String,
}

trait OpenListResponseExt {
    async fn check_openlist_response(self) -> Result<()>;
}

impl OpenListResponseExt for reqwest::Response {
    async fn check_openlist_response(self) -> Result<()> {
        let status = self.status();
        let text = self.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("OpenList 请求失败：{} {}", status, text);
        }
        if text.trim().is_empty() {
            return Ok(());
        }
        let value = serde_json::from_str::<OpenListResponse<serde_json::Value>>(&text)
            .with_context(|| format!("解析 OpenList 响应失败：{}", text))?;
        ensure_openlist_success(value.code, &value.message)
    }
}

fn ensure_openlist_success(code: i32, message: &str) -> Result<()> {
    if matches!(code, 0 | 200) {
        Ok(())
    } else {
        bail!("OpenList API 返回错误：{} {}", code, message)
    }
}

fn percent_encode(input: &str) -> String {
    input
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}
