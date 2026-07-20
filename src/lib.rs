mod csv_process;
mod downloader;
mod log_process;
mod web_parser;

use colored::*;
use downloader::start_download;
use std::error::Error;
use std::io::Write;
use url::Url;

/// 读取用户输入，使用 print! 让光标停留在同一行
fn read_line(prompt: &str) -> String {
    print!("{}", prompt);
    // 【核心修复】：手动刷新标准输出缓冲区，防止 Windows 终端下文字不显示
    std::io::stdout().flush().expect("刷新输出失败");

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("读取输入失败");
    input.trim().to_string()
}

/// 处理网址解析与下载
fn url_parser_csv() -> Result<(), Box<dyn Error>> {
    let input = read_line(&format!("{} ", "🔗 请输入URL:".cyan()));

    // 1. 空输入保护
    if input.is_empty() {
        println!("{}", "⚠️ 未输入任何内容，已取消操作。".yellow());
        return Ok(());
    }

    // 2. 核心校验：判断是否为合法的 URL
    let parsed_url = match Url::parse(&input) {
        Ok(url) => url, // 解析成功，获取 Url 对象
        Err(_) => {
            println!(
                "{} {}",
                "❌ 格式错误:".red().bold(),
                "您输入的不是一个有效的网址，请检查后重试。"
            );
            return Ok(()); // 校验失败，优雅地返回，不中断主循环
        }
    };

    // 3. 提取标准格式的 URL 字符串
    let final_url = parsed_url.to_string();
    println!("{} {}", "🌐 识别到网址:".green(), final_url);
    println!("{}", "⏳ 正在解析网页，请稍候...".yellow());

    // 4. 调用解析器
    let res = web_parser::mrds_parser(&final_url);

    match res {
        Ok(_) => {
            println!("{}", "✅ 网页解析完成，准备开始下载...".green());
            start_download();
            Ok(())
        }
        Err(e) => {
            println!("{} {}", "❌ 解析失败，原因:".red().bold(), e);
            Err(e)
        }
    }
}

/// 主流程入口（暴露给 main.rs 调用）
pub fn run() {
    println!("{}", "🚀 M3U8 视频下载器已成功启动！".bright_green().bold());

    loop {
        println!(
            "\n{}",
            "╔═══════════════════════════════════════╗".bright_cyan()
        );
        println!(
            "{}",
            "║       🚀 M3U8 视频下载器 v1.4         ║".bright_cyan()
        );
        println!(
            "{}",
            "╚═══════════════════════════════════════╝".bright_cyan()
        );
        println!();
        println!("  {}", "【1】 🌐 通过网址解析并下载 (适配特定网站)".green());
        println!("  {}", "【2】 📥 直接下载 DownloadList 内容".green());
        println!("  {}", "【0】 🚪 退出程序".red());
        println!();

        let choice = read_line(&format!("{} ", "👉 请输入选项:".yellow().bold()));

        match choice.as_str() {
            "1" => {
                if let Err(e) = url_parser_csv() {
                    println!("{} {}", "⚠️ 操作中断:".yellow(), e);
                }
            }
            "2" => {
                println!("{}", "⏳ 正在读取本地列表，准备下载...".yellow());
                start_download();
            }
            "0" => {
                println!(
                    "\n{} 👋 感谢使用，祝你生活愉快！\n",
                    "再见!".bright_green().bold()
                );
                break;
            }
            _ => {
                println!(
                    "\n{} 请输入有效的选项 (0, 1, 2)\n",
                    "⚠️ 错误输入:".red().bold()
                );
            }
        }
    }
}
