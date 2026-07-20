use std::fs;
use std::sync::Mutex;

static CSV_MUTEX: Mutex<()> = Mutex::new(());
const FILE_PATH: &str = "./DownloadList.csv";

// ---------------------------- CSV 操作 ----------------------------
pub fn analysis_csv() -> (Vec<String>, Vec<String>) {
    let content = fs::read_to_string(FILE_PATH).expect("无法读取 DownloadList.csv");
    let mut titles = Vec::new();
    let mut urls = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 2 {
            titles.push(parts[0].trim().to_string());
            urls.push(parts[1].trim().to_string());
        }
    }
    (titles, urls)
}

pub fn remove_line(remove_title: &str) {
    let _lock = CSV_MUTEX.lock().unwrap();
    let content = fs::read_to_string(FILE_PATH).expect("无法读取 DownloadList.csv");
    let new_lines: Vec<String> = content
        .lines()
        .filter(|line| {
            let line = line.trim();
            if line.is_empty() {
                return false;
            }
            let parts: Vec<&str> = line.split(',').collect();
            if parts.is_empty() {
                return false;
            }
            parts[0].trim() != remove_title
        })
        .map(String::from)
        .collect();

    let new_content = new_lines.join("\n");
    fs::write(FILE_PATH, new_content).expect("无法写入 DownloadList.csv");
}
