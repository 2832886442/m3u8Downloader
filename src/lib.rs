mod csv_process;
mod downloader;
mod log_process;
mod web_parser;
use downloader::start_download;
use std::error::Error;

fn read_line(prompt: &str) -> String {
    println!("{}", prompt);
    let mut choice = String::new();
    std::io::stdin().read_line(&mut choice).expect("读取失败");
    choice.trim().to_string()
}

fn url_parser_csv() -> Result<(), Box<dyn Error>> {
    let url = read_line("请输入URL:");
    let res = web_parser::mrds_parser(&url);
    match res {
        Ok(_) => {
            start_download();
            Ok(())
        }
        Err(e) => {
            println!("解析失败，原因: {}", e);
            Err(e)
        }
    }
}

// ---------------------------- 主流程 ----------------------------
pub fn run() {
    loop {
        println!("------------欢迎打开m3u8下载器---------");
        println!("请输入你的需求:");
        println!("1. 通过网址进行下载 (只适配某些网站)");
        println!("2. 直接下载 DownloadList 内的指定内容");
        println!("0. 退出程序");

        let choice: &str = &read_line("选择:");

        match choice {
            "1" => {
                // 调用解析函数，并优雅地处理返回的 Result
                if let Err(e) = url_parser_csv() {
                    // 如果解析失败，打印具体的错误原因，但不中断主循环
                    println!("❌ 操作失败: {}", e);
                }
            }
            "2" => {
                start_download();
            }
            "0" => {
                println!("👋 感谢使用，再见！");
                break; // 退出主循环
            }
            _ => {
                println!("⚠️ 错误输入，请输入 0, 1 或 2！");
            }
        }

        println!(); // 打印一个空行，让控制台输出更美观
    }
}
