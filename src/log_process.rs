use super::csv_process::remove_line;
use chrono::Local;
use std::fs::{self, File};
use std::io::Write;

const OUTPUT_DIR: &str = "./Downloads";

// ---------------------------- 日志与错误处理 ----------------------------
pub fn create_log(returncode: i32, output: &str, title: &str) {
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
