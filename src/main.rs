// src/main.rs

use chrono::Local;
use regex::Regex;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;

// ---------------------------- 常量与全局锁 ----------------------------
const FILE_PATH: &str = "./DownloadList.csv";
const N_M3U8DL_CLI: &str = "./N_m3u8DL_CLI/N_m3u8DL-CLI_v3.0.2.exe";
const OUTPUT_DIR: &str = "./Downloads";
const MAX_WORKERS: usize = 5;

static PRINT_LOCK: Mutex<()> = Mutex::new(());
static CSV_MUTEX: Mutex<()> = Mutex::new(());

// ---------------------------- CSV 操作 ----------------------------
fn analysis_csv() -> (Vec<String>, Vec<String>) {
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

fn remove_line(remove_title: &str) {
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

// ---------------------------- 日志与错误处理 ----------------------------
fn create_log(returncode: i32, output: &str, title: &str) {
    let mut has_error = returncode != 0;
    let mut error_message = String::new();

    if returncode != 0 {
        error_message.push_str(&format!("下载失败，错误码：{}\n", returncode));
        error_message.push_str("STDOUT/STDERR 合并输出:\n");
        error_message.push_str(output);
    } else {
        let read_dir = fs::read_dir(OUTPUT_DIR).expect("无法读取输出目录");
        let file_found = read_dir.filter_map(Result::ok).any(|entry| {
            let path = entry.path();
            path.is_file()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|name| name.starts_with(title))
                    .unwrap_or(false)
        });

        if !file_found {
            has_error = true;
            error_message.push_str("程序返回成功，但未找到生成的文件。\n");
            error_message.push_str("STDOUT/STDERR 合并输出:\n");
            error_message.push_str(output);
        } else {
            println!("下载成功！文件保存在：{}/{}.*", OUTPUT_DIR, title);
            remove_line(title);
            println!("已自动删除已完成的会话。");
            return;
        }
    }

    if has_error {
        let log_dir = "./Logs";
        fs::create_dir_all(log_dir).expect("无法创建日志目录");
        let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S-%f").to_string();
        let log_filename = format!("{}/{}.log", log_dir, timestamp);
        let mut log_file = File::create(&log_filename).expect("无法创建日志文件");
        writeln!(log_file, "========== 异常日志 ==========").unwrap();
        writeln!(
            log_file,
            "时间: {}",
            Local::now().format("%Y-%m-%d %H:%M:%S.%f")
        )
        .unwrap();
        writeln!(log_file, "任务名称: {}", title).unwrap();
        writeln!(log_file, "返回码: {}", returncode).unwrap();
        writeln!(log_file, "错误信息:\n{}", error_message).unwrap();
        writeln!(log_file, "===============================").unwrap();
        println!("下载出现问题，详细日志已保存至：{}", log_filename);
    }
}

// ---------------------------- 单个下载任务 ----------------------------
fn download_single(title: &str, url: &str) {
    let mut child = Command::new(N_M3U8DL_CLI)
        .arg(url)
        .arg("--workDir")
        .arg(OUTPUT_DIR)
        .arg("--saveName")
        .arg(title)
        .arg("--enableDelAfterDone")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("无法启动下载程序");

    let stdout = child.stdout.take().expect("无法获取 stdout");
    let stderr = child.stderr.take().expect("无法获取 stderr");

    let output_arc = std::sync::Arc::new(Mutex::new(String::new()));
    let progress_regex = Regex::new(r"Progress:\s*(\d+)/(\d+)\s*\(([\d.]+)%\)").unwrap();

    // ---- 读取 stdout 的线程 ----
    let out_handle = {
        let output = output_arc.clone();
        let title = title.to_string();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut last_downloaded = -1;
            let mut buf = Vec::new();

            // 使用 Read::bytes 显式调用，并传入 &mut reader
            for byte in Read::bytes(&mut reader) {
                let b = match byte {
                    Ok(b) => b,
                    Err(_) => break,
                };

                if b == b'\r' || b == b'\n' {
                    if !buf.is_empty() {
                        let line_bytes = std::mem::take(&mut buf);
                        if let Ok(line) = String::from_utf8(line_bytes) {
                            {
                                let mut out = output.lock().unwrap();
                                out.push_str(&line);
                                out.push('\n');
                            }
                            if let Some(caps) = progress_regex.captures(&line) {
                                let downloaded: i32 = caps[1].parse().unwrap_or(0);
                                let total: i32 = caps[2].parse().unwrap_or(0);
                                let percent = &caps[3];
                                if downloaded > last_downloaded {
                                    last_downloaded = downloaded;
                                    let _lock = PRINT_LOCK.lock().unwrap();
                                    println!(
                                        "[{}] 进度: {}/{} ({}%)",
                                        title, downloaded, total, percent
                                    );
                                }
                            }
                        }
                    }
                } else {
                    buf.push(b);
                }
            }

            // 处理剩余内容
            if !buf.is_empty() {
                if let Ok(line) = String::from_utf8(buf) {
                    {
                        let mut out = output.lock().unwrap();
                        out.push_str(&line);
                        out.push('\n');
                    }
                    if let Some(caps) = progress_regex.captures(&line) {
                        let downloaded: i32 = caps[1].parse().unwrap_or(0);
                        let total: i32 = caps[2].parse().unwrap_or(0);
                        let percent = &caps[3];
                        if downloaded > last_downloaded {
                            let _lock = PRINT_LOCK.lock().unwrap();
                            println!("[{}] 进度: {}/{} ({}%)", title, downloaded, total, percent);
                        }
                    }
                }
            }
        })
    };

    // stderr 读取（保持简单，使用 lines）
    let err_handle = {
        let output = output_arc.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    let mut out = output.lock().unwrap();
                    out.push_str(&line);
                    out.push('\n');
                }
            }
        })
    };

    let status = child.wait().expect("子进程等待失败");
    out_handle.join().unwrap();
    err_handle.join().unwrap();

    let full_output = {
        let out = output_arc.lock().unwrap();
        out.clone()
    };

    create_log(status.code().unwrap_or(-1), &full_output, title);
}

// ---------------------------- 主流程 ----------------------------
fn main() {
    let (titles, urls) = analysis_csv();
    if titles.is_empty() {
        println!("DownloadList.csv 为空或格式不正确，退出。");
        return;
    }
    println!("开始下载任务...");

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(MAX_WORKERS)
        .build()
        .expect("创建线程池失败");

    pool.install(|| {
        use rayon::prelude::*;
        let pairs: Vec<(&String, &String)> = titles.iter().zip(urls.iter()).collect();
        pairs.par_iter().for_each(|(title, url)| {
            download_single(title, url);
        });
    });
}
