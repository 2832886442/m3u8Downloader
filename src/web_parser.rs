use reqwest::blocking::{self, Client};
use reqwest::header::{ORIGIN, REFERER, USER_AGENT};
use serde_json::Value;
use soup::prelude::*;
use std::error::Error;
use std::fs::OpenOptions;
use std::path::Path;
use std::time::Duration;
use url::Url;

/// 解析视频页面，提取标题和 URL，生成 CSV
pub fn parser1(url: &str) -> Result<(), Box<dyn Error>> {
    // 1. 优雅地处理 page_list 的返回结果
    let url_list = match page_list(url) {
        Ok(res) if !res.is_empty() => res,
        _ => vec![url.to_string()],
    };
    println!("已解析到网址如下:\n{}", url_list.join("\n"));
    println!("共为 {} 个页面\n", url_list.len());

    // 2. 准备收集所有页面的数据
    let mut all_titles: Vec<String> = Vec::new();
    let mut all_urls: Vec<String> = Vec::new();
    let mut failed_urls: Vec<String> = Vec::new();

    // 3. 创建全局复用的 HTTP Client，大幅提升性能
    let client = Client::new();

    // 4. 遍历解析出的所有 URL，提取数据
    for current_url in &url_list {
        // 提取当前 URL 的域名，用于动态设置请求头
        let host = Url::parse(current_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_default();

        // 增加重试机制：最多尝试 3 次
        let mut success = false;
        for attempt in 1..=3 {
            // 构建带有必要标头的请求
            let request_result = client
                .get(current_url)
                .header(
                    USER_AGENT,
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
                )
                .header(REFERER, format!("https://{}/", host))
                .header(ORIGIN, format!("https://{}", host))
                .send();

            match request_result {
                Ok(response) => {
                    match response.text() {
                        Ok(content) => {
                            let soup = Soup::new(&content);
                            let mut title_list: Vec<String> = Vec::new();
                            let mut final_url_list: Vec<String> = Vec::new();

                            let player_configs: Vec<_> =
                                soup.tag("div").class("dplayer").find_all().collect();

                            if player_configs.is_empty() {
                                println!(
                                    "⚠️ 页面 [{}] player_config_list 为空，无法解析.",
                                    current_url
                                );
                            } else {
                                for node in &player_configs {
                                    // 提取标题并替换非法字符
                                    let title = node
                                        .get("data-video_title")
                                        .unwrap_or("未知标题".into())
                                        .replace("/", ".")
                                        .trim()
                                        .to_string();

                                    // 提取 URL
                                    let mut video_url_opt = None;
                                    if let Some(raw_config) = node.get("data-config") {
                                        if let Ok(config) =
                                            serde_json::from_str::<Value>(raw_config.as_str())
                                        {
                                            if let Some(video_url) = config
                                                .get("video")
                                                .and_then(|v| v.get("url"))
                                                .and_then(|u| u.as_str())
                                            {
                                                video_url_opt = Some(video_url.to_string());
                                            }
                                        }
                                    }

                                    if let Some(video_url) = video_url_opt {
                                        title_list.push(title);
                                        final_url_list.push(video_url);
                                    }
                                }
                            }

                            // 打印当前页面的解析详情
                            println!(
                                "✅ 页面 [{}] 共找到 {} 个视频",
                                current_url,
                                title_list.len()
                            );
                            for (t, u) in title_list.iter().zip(final_url_list.iter()) {
                                println!("   🎬 标题: {}", t);
                                println!("   🔗 链接: {}", u);
                            }

                            // 汇总到总列表中
                            if title_list.len() == final_url_list.len() && !title_list.is_empty() {
                                all_titles.extend(title_list);
                                all_urls.extend(final_url_list);
                            } else {
                                println!("❌ 页面 [{}] 标题和 URL 数量不匹配或无数据", current_url);
                            }

                            success = true;
                            break; // 成功则跳出重试循环
                        }
                        Err(e) => println!("⚠️ 第 {} 次尝试读取文本失败: {}", attempt, e),
                    }
                }
                Err(e) => println!("⚠️ 第 {} 次尝试请求失败: {}", attempt, e),
            }

            // 如果还没成功，且不是最后一次尝试，则等待 1 秒后重试
            if !success && attempt < 3 {
                println!("⏳ 等待 1 秒后重试...");
                std::thread::sleep(Duration::from_secs(1));
            }
        }

        // 如果 3 次都失败了，记录到 failed_urls
        if !success {
            println!("❌ 页面请求失败，已跳过: {}", current_url);
            failed_urls.push(current_url.clone());
        }
    }

    // 5. 所有页面遍历完毕后，一次性生成完整的 CSV
    if !all_titles.is_empty() {
        println!("\n🎉 所有页面解析完成，正在生成 DownloadList.csv 中...");
        create_csv(&all_titles, &all_urls)?;
    } else {
        println!("\n⚠️ 未能提取到任何有效的视频数据，跳过 CSV 生成。");
    }

    // 6. 输出失败记录汇总
    if !failed_urls.is_empty() {
        println!(
            "\n⚠️ 以下 {} 个页面在 3 次重试后依然失败:",
            failed_urls.len()
        );
        for url in &failed_urls {
            println!("  - {}", url);
        }
    }

    Ok(())
}

fn page_list(url: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let response = blocking::get(url)?;
    let content = response.text()?;
    let soup = Soup::new(&content);

    let parsed = Url::parse(url)?;
    let prefix = format!("https://{}", parsed.host_str().unwrap_or(""));

    let url_list: Vec<String> = soup
        .tag("meta")
        .attr("itemprop", "url mainEntityOfPage")
        .find_all()
        .map(|element| {
            let suffix = element.get("content").unwrap_or("".into());
            format!("{}{}", prefix, suffix)
        })
        .collect();

    if !url_list.is_empty() {
        Ok(url_list)
    } else {
        Err("无法解析到分页列表".into())
    }
}

/// 示例 CSV 生成函数（保留原有逻辑）
fn create_csv(titles: &[String], urls: &[String]) -> Result<(), Box<dyn Error>> {
    let file_path = "DownloadList.csv";
    let is_new_file = !Path::new(file_path).exists();
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)?;
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);
    if is_new_file {
        wtr.write_record(&["Title", "URL"])?;
    }
    for (title, url) in titles.iter().zip(urls.iter()) {
        wtr.write_record(&[title, url])?;
    }
    wtr.flush()?;
    Ok(())
}
