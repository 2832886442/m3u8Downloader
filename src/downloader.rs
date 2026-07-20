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
    // 1. 设置初始样式
    let spinner_style = ProgressStyle::default_spinner()
        .template("{msg:.bold} {spinner:.green} ⏳ 正在准备下载...")
        .unwrap();
    pb.set_style(spinner_style);
    pb.set_message(title.to_string());

    // 【关键修复】：初始状态直接设为 Hidden，防止在获取到真实进度前占用屏幕位置或闪烁
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

    // 2. 提取闭包处理进度
    let process_line = move |line: &str, output: &Arc<Mutex<String>>, pb: &ProgressBar| {
        output.lock().unwrap().push_str(line);
        output.lock().unwrap().push('\n');

        if let Some(caps) = progress_regex.captures(line) {
            let downloaded: u64 = caps[1].parse().unwrap_or(0);
            let total: u64 = caps[2].parse().unwrap_or(0);

            // 【关键修复】：从 Hidden 切换到 Visible 的逻辑
            if total > 0 && pb.is_hidden() {
                let bar_style = ProgressStyle::default_bar()
                    .template(
                        "{msg:.bold} [{bar:30.cyan/blue}] {pos}/{len} ({percent}%, ETA: {eta})",
                    )
                    .unwrap()
                    .progress_chars("=>-");
                pb.set_style(bar_style);
                pb.set_length(total);
                // 此时再将其挂载到标准输出，MultiProgress 会自动将其放入预留的正确位置
                pb.set_draw_target(ProgressDrawTarget::stdout());
            }

            if total > 0 {
                pb.set_position(downloaded);
            }
        }
    };

    // 3. 逐字节读取 stdout (保持原有逻辑不变)
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

    // 5. 完成进度条
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

    // 创建 MultiProgress 管理器
    let multi = MultiProgress::new();

    // 【核心修复】：在主线程中预先创建所有进度条并添加到 MultiProgress
    // 这样做可以锁定每个进度条在屏幕上的垂直位置，防止多线程竞争导致的乱序
    let progress_bars: Vec<ProgressBar> = titles
        .iter()
        .map(|_| {
            // 初始化为一个占位用的进度条，长度为0或1均可，稍后在 download_single 中会重置
            let pb = multi.add(ProgressBar::new(0));
            pb.set_draw_target(ProgressDrawTarget::hidden()); // 初始全部隐藏，等解析出数据再显示
            pb
        })
        .collect();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(MAX_WORKERS)
        .build()
        .expect("创建线程池失败");

    pool.install(|| {
        use rayon::prelude::*;

        // 使用 zip 将标题、URL和预创建的进度条绑定在一起
        titles
            .par_iter()
            .zip(urls.par_iter())
            .zip(progress_bars.par_iter())
            .for_each(|((title, url), pb)| {
                // clone 进度条以传入函数
                download_single(title, url, pb.clone());
            });
    });
}
