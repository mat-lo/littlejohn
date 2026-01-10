//! Real-Debrid API client

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;

const BASE_URL: &str = "https://api.real-debrid.com/rest/1.0";

/// A file in a torrent
#[derive(Debug, Clone)]
pub struct TorrentFile {
    pub id: u32,
    pub path: String,
    pub bytes: u64,
    pub selected: bool,
}

impl TorrentFile {
    /// Get just the filename from the path
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Human-readable size
    pub fn size_str(&self) -> String {
        let mut size = self.bytes as f64;
        for unit in ["B", "KB", "MB", "GB", "TB"] {
            if size < 1024.0 {
                return format!("{:.1} {}", size, unit);
            }
            size /= 1024.0;
        }
        format!("{:.1} PB", size)
    }
}

/// Real-Debrid API response for adding magnet
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AddMagnetResponse {
    id: String,
    uri: String,
}

/// Real-Debrid torrent file info from API
#[derive(Debug, Deserialize)]
struct ApiTorrentFile {
    id: u32,
    path: String,
    bytes: u64,
    selected: Option<u8>,
}

/// Real-Debrid torrent info
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TorrentInfo {
    id: String,
    status: String,
    files: Option<Vec<ApiTorrentFile>>,
    links: Option<Vec<String>>,
    progress: Option<f64>,
    speed: Option<u64>,
    seeders: Option<u32>,
}

/// Real-Debrid unrestrict response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct UnrestrictResponse {
    filename: String,
    download: String,
    filesize: Option<u64>,
}

/// Real-Debrid error response
#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
    error_code: Option<i32>,
}

/// Real-Debrid user info
#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub expiration: String,
    #[serde(rename = "type")]
    pub account_type: String,
}

/// Real-Debrid API client
#[derive(Debug, Clone)]
pub struct RealDebridClient {
    api_token: String,
    client: reqwest::Client,
}

impl RealDebridClient {
    /// Create a new Real-Debrid client
    pub fn new() -> Result<Self> {
        let api_token = env::var("RD_API_TOKEN")
            .map_err(|_| anyhow!("RD_API_TOKEN not set in environment"))?;

        if api_token.is_empty() || api_token == "your_api_token_here" {
            return Err(anyhow!("RD_API_TOKEN not configured"));
        }

        Ok(Self {
            api_token,
            client: reqwest::Client::new(),
        })
    }

    /// Make an authenticated request
    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        endpoint: &str,
        data: Option<HashMap<&str, &str>>,
    ) -> Result<T> {
        let url = format!("{}{}", BASE_URL, endpoint);

        let request = match method {
            "GET" => self.client.get(&url),
            "POST" => {
                let mut req = self.client.post(&url);
                if let Some(d) = data {
                    req = req.form(&d);
                }
                req
            }
            "DELETE" => self.client.delete(&url),
            _ => return Err(anyhow!("Unsupported method: {}", method)),
        };

        let response = request
            .header("Authorization", format!("Bearer {}", self.api_token))
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?;

        let status = response.status();

        if status.as_u16() == 204 {
            // No content - return empty object
            return serde_json::from_str("{}").map_err(|e| anyhow!("JSON parse error: {}", e));
        }

        let text = response.text().await?;

        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<ErrorResponse>(&text) {
                return Err(anyhow!("Real-Debrid error: {} (code: {:?})", err.error, err.error_code));
            }
            return Err(anyhow!("Real-Debrid error: {} - {}", status, text));
        }

        serde_json::from_str(&text).map_err(|e| anyhow!("JSON parse error: {} - {}", e, text))
    }

    /// Get current user info
    pub async fn get_user(&self) -> Result<UserInfo> {
        self.request("GET", "/user", None).await
    }

    /// Add a magnet link
    async fn add_magnet(&self, magnet: &str) -> Result<String> {
        let mut data = HashMap::new();
        data.insert("magnet", magnet);

        let response: AddMagnetResponse = self.request("POST", "/torrents/addMagnet", Some(data)).await?;
        Ok(response.id)
    }

    /// Get torrent info
    async fn get_torrent_info(&self, torrent_id: &str) -> Result<TorrentInfo> {
        let endpoint = format!("/torrents/info/{}", torrent_id);
        self.request("GET", &endpoint, None).await
    }

    /// Select files to download
    async fn select_files(&self, torrent_id: &str, files: &str) -> Result<()> {
        let endpoint = format!("/torrents/selectFiles/{}", torrent_id);
        let mut data = HashMap::new();
        data.insert("files", files);

        let _: serde_json::Value = self.request("POST", &endpoint, Some(data)).await?;
        Ok(())
    }

    /// Unrestrict a link to get direct download URL
    async fn unrestrict_link(&self, link: &str) -> Result<UnrestrictResponse> {
        let mut data = HashMap::new();
        data.insert("link", link);

        self.request("POST", "/unrestrict/link", Some(data)).await
    }

    /// Delete a torrent
    pub async fn delete_torrent(&self, torrent_id: &str) -> Result<()> {
        let endpoint = format!("/torrents/delete/{}", torrent_id);
        let _: serde_json::Value = self.request("DELETE", &endpoint, None).await?;
        Ok(())
    }

    /// Add a magnet and get the list of files
    pub async fn get_torrent_files(&self, magnet: &str) -> Result<(String, Vec<TorrentFile>)> {
        let torrent_id = self.add_magnet(magnet).await?;

        // Wait for files to be available
        for _ in 0..30 {
            let info = self.get_torrent_info(&torrent_id).await?;

            match info.status.as_str() {
                "waiting_files_selection" => {
                    let files = info
                        .files
                        .unwrap_or_default()
                        .into_iter()
                        .map(|f| TorrentFile {
                            id: f.id,
                            path: f.path,
                            bytes: f.bytes,
                            selected: f.selected.unwrap_or(0) == 1,
                        })
                        .collect();

                    return Ok((torrent_id, files));
                }
                "magnet_error" => {
                    let _ = self.delete_torrent(&torrent_id).await;
                    return Err(anyhow!("Invalid magnet link"));
                }
                _ => {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }

        let _ = self.delete_torrent(&torrent_id).await;
        Err(anyhow!("Timeout waiting for magnet to resolve"))
    }

    /// Download specific files from a torrent
    pub async fn download_selected_files(
        &self,
        torrent_id: &str,
        file_ids: &[u32],
    ) -> Result<Vec<(String, String)>> {
        self.download_selected_files_with_callback(torrent_id, file_ids, |_| {}).await
    }

    /// Download specific files from a torrent with status callback
    pub async fn download_selected_files_with_callback<F>(
        &self,
        torrent_id: &str,
        file_ids: &[u32],
        mut on_status: F,
    ) -> Result<Vec<(String, String)>>
    where
        F: FnMut(&str),
    {
        // Select the specified files
        let files_str = file_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");

        on_status("Selecting files...");
        self.select_files(torrent_id, &files_str).await?;

        // Wait for torrent to be ready (5 minutes max)
        let wait_timeout = 300;
        let interval = 2;
        let mut elapsed = 0;

        while elapsed < wait_timeout {
            let info = self.get_torrent_info(torrent_id).await?;

            match info.status.as_str() {
                "downloaded" => {
                    on_status("Unrestricting links...");
                    let links = info.links.unwrap_or_default();
                    if links.is_empty() {
                        return Err(anyhow!("No download links available"));
                    }

                    // Unrestrict all links
                    let mut downloads = Vec::new();
                    for (i, link) in links.iter().enumerate() {
                        on_status(&format!("Unrestricting link {}/{}...", i + 1, links.len()));
                        let unrestricted = self.unrestrict_link(link).await?;
                        downloads.push((unrestricted.filename, unrestricted.download));
                    }

                    return Ok(downloads);
                }
                "error" | "dead" | "magnet_error" => {
                    return Err(anyhow!("Torrent failed with status: {}", info.status));
                }
                status => {
                    let progress = info.progress.unwrap_or(0.0);
                    let speed = info.speed.unwrap_or(0);
                    let seeders = info.seeders.unwrap_or(0);

                    let speed_str = if speed > 0 {
                        let mb_s = speed as f64 / 1_000_000.0;
                        format!(" {:.1} MB/s", mb_s)
                    } else {
                        String::new()
                    };

                    on_status(&format!(
                        "RD {}: {:.0}%{} ({} seeders)",
                        status, progress, speed_str, seeders
                    ));
                    tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                    elapsed += interval;
                }
            }
        }

        Err(anyhow!("Timeout waiting for torrent"))
    }
}
