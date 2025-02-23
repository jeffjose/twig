use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Debug)]
pub enum CacheError {
    IoError(std::io::Error),
    SerdeError(serde_json::Error),
    DirectoryError(String),
    TimeError(String),
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheError::IoError(e) => write!(f, "IO error: {}", e),
            CacheError::SerdeError(e) => write!(f, "Serialization error: {}", e),
            CacheError::DirectoryError(e) => write!(f, "Directory error: {}", e),
            CacheError::TimeError(e) => write!(f, "Time error: {}", e),
        }
    }
}

impl Error for CacheError {}

impl From<std::io::Error> for CacheError {
    fn from(err: std::io::Error) -> Self {
        CacheError::IoError(err)
    }
}

impl From<serde_json::Error> for CacheError {
    fn from(err: serde_json::Error) -> Self {
        CacheError::SerdeError(err)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheEntry<T: Clone> {
    pub data: T,
    #[serde(with = "system_time_serde")]
    pub cached_at: SystemTime,
}

impl<T: Clone> CacheEntry<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            cached_at: SystemTime::now(),
        }
    }

    pub fn age(&self) -> Result<Duration, CacheError> {
        SystemTime::now()
            .duration_since(self.cached_at)
            .map_err(|e| CacheError::TimeError(format!("Time went backwards: {}", e)))
    }
}

// Serialization helper for SystemTime
mod system_time_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = time
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0));
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(UNIX_EPOCH + Duration::from_secs(secs))
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct GlobalCache {
    pub power: Option<CacheEntry<crate::power::BatteryInfo>>,
    pub hostname: Option<CacheEntry<String>>,
    pub ip: Option<CacheEntry<std::net::IpAddr>>,
}

impl GlobalCache {
    pub fn load() -> Result<Self, CacheError> {
        let cache_path = get_cache_path()?;
        if !cache_path.exists() {
            return Ok(Self::default());
        }

        let cache_content = fs::read_to_string(&cache_path)?;
        serde_json::from_str(&cache_content).map_err(Into::into)
    }

    pub fn save(&self) -> Result<(), CacheError> {
        let cache_path = get_cache_path()?;

        // Ensure cache directory exists
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let cache_content = serde_json::to_string(self)?;
        fs::write(&cache_path, cache_content)?;
        Ok(())
    }

    pub fn get_power(&self, cache_duration: u64) -> Option<&crate::power::BatteryInfo> {
        self.power.as_ref().and_then(|entry| {
            entry
                .age()
                .ok()
                .filter(|age| *age < Duration::from_secs(cache_duration))
                .map(|_| &entry.data)
        })
    }

    pub fn set_power(&mut self, info: crate::power::BatteryInfo) {
        self.power = Some(CacheEntry::new(info));
    }

    #[allow(dead_code)]
    pub fn get_hostname(&self, cache_duration: u64) -> Option<&String> {
        self.hostname.as_ref().and_then(|entry| {
            entry
                .age()
                .ok()
                .filter(|age| *age < Duration::from_secs(cache_duration))
                .map(|_| &entry.data)
        })
    }

    #[allow(dead_code)]
    pub fn set_hostname(&mut self, hostname: String) {
        self.hostname = Some(CacheEntry::new(hostname));
    }

    #[allow(dead_code)]
    pub fn get_ip(&self, cache_duration: u64) -> Option<&std::net::IpAddr> {
        self.ip.as_ref().and_then(|entry| {
            entry
                .age()
                .ok()
                .filter(|age| *age < Duration::from_secs(cache_duration))
                .map(|_| &entry.data)
        })
    }

    #[allow(dead_code)]
    pub fn set_ip(&mut self, ip: std::net::IpAddr) {
        self.ip = Some(CacheEntry::new(ip));
    }
}

fn get_cache_path() -> Result<PathBuf, CacheError> {
    BaseDirs::new()
        .map(|base_dirs| base_dirs.cache_dir().join("twig").join("cache.json"))
        .ok_or_else(|| {
            CacheError::DirectoryError("Could not determine cache directory".to_string())
        })
}
