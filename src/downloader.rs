use crate::csv_process::analysis_csv;
use crate::log_process::create_log;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use regex::Regex;
use std::io::{BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

const MAX_WORKERS: usize = 5;
const N_M3U8DL_CLI: &str = "./N_m3u8DL_CLI/N_m3u8DL-CLI_v3.0.2.exe";
const OUTPUT_DIR: &str = "./Downloads";

/// 单个下载任务
fn download_single(title: &str, url: &str, pb: ProgressBar) {
    // 1. 【关键修复】初始状态使用隐藏的 Spinner，避免一开始显示 100% 或空白
    let spinner_style = ProgressStyle::default_spinner()
        .template("{msg:.bold} {spinner:.green} ⏳ 正在准备下载...")
        .unwrap();
    pb.set_style(spinner_style);
    pb.set_message(title.to_string());
    // 将进度条设为隐藏状态，等待获取到真实长度后再显示
    pb.set_draw_target(ProgressDrawTarget::hidden());

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

    let output_arc = Arc::new(Mutex::new(String::new()));
    let progress_regex = Regex::new(r"Progress:\s*(\d+)/(\d+)\s*\(([\d.]+)%\)").unwrap();

    // 2. 提取闭包处理进度，使用 move 解决生命周期问题
    let process_line = move |line: &str, output: &Arc<Mutex<String>>, pb: &ProgressBar| {
        // 记录日志
        output.lock().unwrap().push_str(line);
        output.lock().unwrap().push('\n');

        // 匹配进度
        if let Some(caps) = progress_regex.captures(line) {
            let downloaded: u64 = caps[1].parse().unwrap_or(0);
            let total: u64 = caps[2].parse().unwrap_or(0);

            // 【核心修复】当获取到真实的总文件数，且进度条还在隐藏状态时，将其显示出来
            if total > 0 && pb.is_hidden() {
                let bar_style = ProgressStyle::default_bar()
                    .template(&format!(
                        "{{msg:.bold}} [{{bar:30.cyan/blue}}] {{pos}}/{{len}} ({{percent}}%, ETA: {{eta}})"
                    ))
                    .unwrap()
                    .progress_chars("=>-");
                pb.set_style(bar_style);
                pb.set_length(total);
                // 将隐藏的目标切换为正常的终端输出
                pb.set_draw_target(ProgressDrawTarget::stdout());
            }

            // 更新当前已下载的文件数
            if total > 0 {
                pb.set_position(downloaded);
            }
        }
    };

    // 3. 逐字节读取 stdout
    let out_handle = {
        let output = output_arc.clone();
        let pb = pb.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut buf = Vec::new();

            for byte in Read::bytes(&mut reader) {
                let b = match byte {
                    Ok(b) => b,
                    Err(_) => break,
                };

                if b == b'\r' || b == b'\n' {
                    if !buf.is_empty() {
                        if let Ok(line) = String::from_utf8(std::mem::take(&mut buf)) {
                            process_line(&line, &output, &pb);
                        }
                    }
                } else {
                    buf.push(b);
                }
            }

            // 处理缓冲区残留
            if !buf.is_empty() {
                if let Ok(line) = String::from_utf8(buf) {
                    process_line(&line, &output, &pb);
                }
            }
        })
    };

    // 4. 读取 stderr
    let err_handle = {
        let output = output_arc.clone();
        thread::spawn(move || {
            let reader = std::io::BufReader::new(stderr);
            for line in std::io::BufRead::lines(reader) {
                if let Ok(line) = line {
                    output.lock().unwrap().push_str(&line);
                    output.lock().unwrap().push('\n');
                }
            }
        })
    };

    let status = child.wait().expect("子进程等待失败");
    out_handle.join().unwrap();
    err_handle.join().unwrap();

    // 5. 根据状态完成进度条
    if status.success() {
        pb.finish_with_message(format!("{} ✔ 下载完成", title));
    } else {
        pb.abandon_with_message(format!("{} ✖ 下载失败", title));
    }

    let full_output = output_arc.lock().unwrap().clone();
    create_log(status.code().unwrap_or(-1), &full_output, title);
}

pub fn start_download() {
    let (titles, urls) = analysis_csv();
    if titles.is_empty() {
        println!("DownloadList.csv 为空或格式不正确，退出。");
        return;
    }

    println!("🚀 开始下载任务...");

    // 创建 MultiProgress 管理器，它负责协调多个进度条的显示，防止互相覆盖
    let multi = MultiProgress::new();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(MAX_WORKERS)
        .build()
        .expect("创建线程池失败");

    pool.install(|| {
        use rayon::prelude::*;
        let pairs: Vec<(&String, &String)> = titles.iter().zip(urls.iter()).collect();

        pairs.par_iter().for_each(|(title, url)| {
            // 为每个任务创建一个独立的进度条，并交给 MultiProgress 管理
            let pb = multi.add(ProgressBar::new(100));
            download_single(title, url, pb);
        });
    });
}
