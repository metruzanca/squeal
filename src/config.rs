use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub fn absolutize_path(path: &str) -> String {
    if path.starts_with("postgres://") || path.starts_with("postgresql://") {
        return path.to_string();
    }
    let p = std::path::Path::new(path);
    if let Ok(canonical) = p.canonicalize() {
        return canonical.to_string_lossy().to_string();
    }
    if p.is_relative() {
        if let Ok(cwd) = std::env::current_dir() {
            return cwd.join(p).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentEntry {
    pub path: String,
    pub connection_type: String,
    pub last_opened: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub recent: Vec<RecentEntry>,
}

impl Config {
    pub fn config_dir() -> PathBuf {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".config").join("squeal")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("recent.toml")
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Config::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("recent.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn add_recent(&mut self, path: &str, connection_type: &str) {
        let path = absolutize_path(path);
        // Remove existing entry with same path if present
        self.recent.retain(|e| e.path != path);

        let entry = RecentEntry {
            path,
            connection_type: connection_type.to_string(),
            last_opened: chrono::Local::now().to_rfc3339(),
        };
        self.recent.insert(0, entry);
        self.recent.truncate(MAX_RECENT);
    }

    pub fn remove_recent(&mut self, index: usize) {
        if index < self.recent.len() {
            self.recent.remove(index);
        }
    }
}
