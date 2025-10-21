use directories::ProjectDirs;
use gethostname::gethostname;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
struct CachedData {
    hostname: String,
    timestamp: u64,
}

fn main() {
    println!("twigd - starting daemon");

    // Get data file path
    let data_path = get_data_file_path();

    // Ensure parent directory exists
    if let Some(parent) = data_path.parent() {
        fs::create_dir_all(parent)
            .expect("Failed to create data directory");
    }

    println!("Cache file: {}", data_path.display());
    println!();

    // Main daemon loop
    let mut countdown = 1;
    loop {
        // Get hostname (this is our cached data)
        let hostname = gethostname()
            .to_string_lossy()
            .to_string();

        // Create cached data structure
        let cached = CachedData {
            hostname,
            timestamp: current_timestamp(),
        };

        // Write to JSON file
        let json = serde_json::to_string_pretty(&cached)
            .expect("Failed to serialize data");

        fs::write(&data_path, json)
            .expect("Failed to write cache file");

        // Print update status with countdown (in-place)
        print!("\rUpdated cache. Next update in {}s...", countdown);
        use std::io::{self, Write};
        io::stdout().flush().unwrap();

        // Sleep for 1 second
        thread::sleep(Duration::from_secs(1));

        countdown -= 1;
        if countdown == 0 {
            countdown = 1;
        }
    }
}

/// Get data file path: ~/.local/share/twig/data.json
fn get_data_file_path() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("", "", "twig") {
        proj_dirs.data_dir().join("data.json")
    } else {
        // Fallback to ~/.local/share/twig/data.json
        let mut path = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        path.push(".local");
        path.push("share");
        path.push("twig");
        path.push("data.json");
        path
    }
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}
