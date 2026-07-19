use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const ADJECTIVES: &[&str] = &[
    "agile", "brave", "bright", "bold", "calm", "clever", "cozy", "crisp",
    "eager", "fancy", "fresh", "gentle", "golden", "grand", "happy", "jolly",
    "keen", "lively", "lucky", "merry", "mild", "nimble", "noble", "perky",
    "plucky", "proud", "quick", "quiet", "rapid", "royal", "sage", "sharp",
    "sleek", "slick", "smart", "snug", "spicy", "spunky", "steady", "sunny",
    "swift", "tidy", "vivid", "warm", "wild", "wise", "witty", "zany", "zesty",
];

const ANIMALS: &[&str] = &[
    "alpaca", "badger", "bison", "capybara", "cheetah", "cobra", "cougar",
    "coyote", "crane", "deer", "dingo", "dolphin", "dove", "eagle", "elk",
    "emu", "falcon", "ferret", "finch", "fox", "gazelle", "gecko", "gibbon",
    "giraffe", "goose", "hare", "hawk", "heron", "hyena", "iguana", "jaguar",
    "koala", "lemur", "leopard", "lion", "llama", "lynx", "magpie", "mink",
    "mongoose", "moose", "newt", "ocelot", "octopus", "osprey", "otter", "owl",
    "panda", "parrot", "pelican", "penguin", "puma", "quail", "rabbit",
    "raccoon", "raven", "robin", "salamander", "salmon", "seal", "shark",
    "sloth", "sparrow", "squirrel", "stork", "swan", "tiger", "toucan",
    "trout", "turkey", "turtle", "walrus", "weasel", "whale", "wolf",
    "wolverine", "wombat", "yak", "zebra",
];

pub fn generate_db_name(seed: &str) -> String {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let hash = hasher.finish();
    let adj = ADJECTIVES[hash as usize % ADJECTIVES.len()];
    let animal = ANIMALS[(hash >> 32) as usize % ANIMALS.len()];
    format!("{}_{}", adj, animal)
}

pub fn detect_databases() -> Vec<RecentEntry> {
    let mut detected = Vec::new();
    if let Ok(url) = std::env::var("DATABASE_URL") {
        if !url.is_empty() {
            detected.push(RecentEntry {
                path: url.clone(),
                name: Some(generate_db_name(&url)),
                connection_type: "env".to_string(),
                last_opened: String::new(),
            });
        }
    }
    detected
}

pub fn censor_connection_string(path: &str) -> String {
    if !path.starts_with("postgres://") && !path.starts_with("postgresql://") {
        return path.to_string();
    }
    let after_scheme = path.find("://").map(|i| i + 3).unwrap_or(0);
    let rest = &path[after_scheme..];
    if let Some(at_pos) = rest.find('@') {
        let userinfo = &rest[..at_pos];
        if let Some(colon_pos) = userinfo.find(':') {
            let user = &userinfo[..colon_pos];
            let censored = format!("{}:****", user);
            return format!("{}{}{}", &path[..after_scheme], censored, &rest[at_pos..]);
        }
    }
    path.to_string()
}

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
    #[serde(default)]
    pub name: Option<String>,
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
        let mut config: Config = toml::from_str(&content)?;
        for entry in &mut config.recent {
            if entry.name.is_none() {
                entry.name = Some(generate_db_name(&entry.path));
            }
        }
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
        self.add_recent_with_name(path, connection_type, None)
    }

    pub fn add_recent_with_name(&mut self, path: &str, connection_type: &str, name: Option<&str>) {
        let path = absolutize_path(path);
        // Preserve existing name if entry already exists
        let existing_name = self
            .recent
            .iter()
            .find(|e| e.path == path)
            .and_then(|e| e.name.clone());
        // Remove existing entry with same path if present
        self.recent.retain(|e| e.path != path);

        let name = name
            .map(|s| s.to_string())
            .or(existing_name)
            .unwrap_or_else(|| generate_db_name(&path));

        let entry = RecentEntry {
            path,
            name: Some(name),
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

    pub fn rename_recent(&mut self, index: usize, new_name: &str) {
        if let Some(entry) = self.recent.get_mut(index) {
            entry.name = Some(new_name.to_string());
        }
    }
}
