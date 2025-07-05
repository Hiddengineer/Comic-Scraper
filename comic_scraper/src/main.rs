use serde::Deserialize;
use std::fs::{create_dir_all, File};
use std::io::{copy, stdin};
use std::path::Path;
use reqwest::Client;
use url::Url;
use scraper::{Html, Selector};

#[derive(Debug, Deserialize)]
struct XkcdComic {
    num: u32,
    title: String,
    img: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    loop {
        println!("\nEnter a comic URL (or type 'exit' to quit):");
        let mut input_url = String::new();
        stdin().read_line(&mut input_url)?;
        let input_url = input_url.trim();

        if input_url.eq_ignore_ascii_case("exit") {
            println!("👋 Goodbye!");
            break;
        }

        let site_name = match get_site_name(input_url) {
            Ok(site) => site,
            Err(e) => {
                eprintln!("❌ Invalid URL: {e}");
                continue;
            }
        };

        let result = match site_name.as_str() {
            "xkcd.com" => {
                let comic = fetch_xkcd_comic(input_url).await;
                match comic {
                    Ok(comic) => {
                        let client = Client::new();
                        let save_path = "Comics/xkcd";
                        create_dir_all(save_path)?;
                        download_comic(&client, &comic.img, &comic.title, comic.num, save_path).await
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to fetch XKCD comic: {e}");
                        continue;
                    }
                }
            }

            "explosm.net" => {
                let comic = fetch_explosm_comic(input_url).await;
                match comic {
                    Ok((title, img_url)) => {
                        let client = Client::new();
                        let save_path = "Comics/explosm";
                        create_dir_all(save_path)?;
                        download_comic(&client, &img_url, &title, 0, save_path).await
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to fetch explosm comic: {e}");
                        continue;
                    }
                }
            }

            "www.smbc-comics.com" | "smbc-comics.com" => {
                let comic = fetch_smbc_comic(input_url).await;
                match comic {
                    Ok((title, img_url)) => {
                        let client = Client::new();
                        let save_path = "Comics/smbc";
                        create_dir_all(save_path)?;
                        download_comic(&client, &img_url, &title, 0, save_path).await
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to fetch SMBC comic: {e}");
                        continue;
                    }
                }
            }

            _ => {
                eprintln!("❌ Unsupported site: {site_name}");
                continue;
            }
        };

        if let Err(e) = result {
            eprintln!("❌ Download failed: {e}");
        }
    }

    Ok(())
}

fn get_site_name(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let parsed = Url::parse(url)?;
    Ok(parsed.host_str().unwrap_or("unknown").to_string())
}

async fn fetch_xkcd_comic(url: &str) -> Result<XkcdComic, Box<dyn std::error::Error>> {
    let comic_id = if url.trim_end_matches('/').ends_with("xkcd.com") {
        "info.0.json".to_string()
    } else {
        let comic_number = url
            .trim_end_matches('/')
            .split('/')
            .filter(|part| !part.is_empty())
            .last()
            .unwrap_or("info.0.json");
        format!("{}/info.0.json", comic_number)
    };

    let api_url = format!("https://xkcd.com/{}", comic_id);
    let client = Client::new();
    let response = client.get(api_url).send().await?;
    let comic: XkcdComic = response.json().await?;
    Ok(comic)
}

async fn fetch_explosm_comic(url: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let client = Client::new();
    let resp = client.get(url).send().await?.text().await?;
    let document = Html::parse_document(&resp);

    let selector = Selector::parse("img").unwrap();
    let img = document.select(&selector)
        .find(|img| {
            img.value().attr("src")
                .map(|src| src.contains("files.explosm.net"))
                .unwrap_or(false)
        });

    if let Some(img) = img {
        let src = img.value().attr("src").ok_or("Missing src")?;
        let full_url = if src.starts_with("//") {
            format!("https:{}", src)
        } else if src.starts_with("http") {
            src.to_string()
        } else {
            format!("https://explosm.net{}", src)
        };

        let title = src.split('/').last().unwrap_or("comic").to_string();
        return Ok((title, full_url));
    }

    Err("Comic image not found".into())
}

async fn fetch_smbc_comic(url: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let client = Client::new();
    let resp = client.get(url).send().await?.text().await?;
    let document = Html::parse_document(&resp);
    let selector = Selector::parse("img#cc-comic").unwrap();

    if let Some(img) = document.select(&selector).next() {
        let src = img.value().attr("src").ok_or("Missing src")?;
        let full_url = if src.starts_with("http") {
            src.to_string()
        } else {
            format!("https://www.smbc-comics.com{}", src)
        };
        let title = img.value().attr("alt").unwrap_or("comic").to_string();
        return Ok((title, full_url));
    }
    Err("Comic image not found".into())
}

async fn download_comic(
    client: &Client,
    img_url: &str,
    title: &str,
    num: u32,
    save_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let ext = img_url.split('.').last().unwrap_or("png");
    let safe_title = sanitize_filename(title);
    let prefix = if num > 0 { format!("{:04} - ", num) } else { "".to_string() };
    let filename = format!("{}{}.{}", prefix, safe_title, ext);
    let path = Path::new(save_dir).join(&filename);

    if path.exists() {
        println!("✅ Already downloaded: {}", filename);
        return Ok(());
    }

    println!("⬇️ Downloading: {}", filename);
    let resp = client.get(img_url).send().await?;
    let mut file = File::create(path)?;
    let bytes = resp.bytes().await?;
    copy(&mut bytes.as_ref(), &mut file)?;

    println!("✅ Saved to: {}/{}", save_dir, filename);
    Ok(())
}

fn sanitize_filename(title: &str) -> String {
    let bad_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    title
        .chars()
        .map(|c| if bad_chars.contains(&c) { '_' } else { c })
        .collect()
}