use aidoku::{
    alloc::{string::ToString, vec, String, Vec},
    prelude::*,
    Chapter, ContentRating, Manga, MangaPageResult, MangaStatus, Page, PageContent, Result, Viewer,
};
use serde_json::Value;

pub fn text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    }
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit()) && value != "0"
}

pub fn artwork_id(url: &str) -> Option<String> {
    let tail = url
        .strip_prefix("https://www.pixiv.net/")
        .or_else(|| url.strip_prefix("https://pixiv.net/"))?;
    let path = tail.split(['?', '#']).next()?;
    let id = path
        .strip_prefix("artworks/")
        .or_else(|| path.strip_prefix("en/artworks/"))
        .or_else(|| path.strip_prefix("ja/artworks/"))
        .or_else(|| path.strip_prefix("zh/artworks/"));
    if let Some(id) = id {
        let id = id.trim_end_matches('/');
        return valid_id(id).then(|| id.into());
    }
    if path == "member_illust.php" {
        let query = tail.split_once('?')?.1.split('#').next()?;
        for param in query.split('&') {
            if let Some(id) = param.strip_prefix("illust_id=") {
                return valid_id(id).then(|| id.into());
            }
        }
    }
    None
}

pub fn is_animation(item: &Value) -> bool {
    item.get("type").and_then(Value::as_str) == Some("ugoira")
        || item.get("illustType").and_then(Value::as_i64) == Some(2)
        || text(&item["illust_type"]) == "2"
}

pub fn is_adult(item: &Value) -> bool {
    item.get("x_restrict")
        .or_else(|| item.get("xRestrict"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        > 0
        || item
            .pointer("/illust_content_type/sexual")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            > 0
}

pub fn manga(item: &Value) -> Option<Manga> {
    if is_animation(item) || item.get("isAdContainer").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let id = text(item.get("id").or_else(|| item.get("illust_id"))?);
    if !valid_id(&id) {
        return None;
    }
    let cover = item
        .pointer("/image_urls/large")
        .or_else(|| item.pointer("/urls/regular"))
        .or_else(|| item.get("url"));
    let author = item
        .pointer("/user/name")
        .or_else(|| item.get("userName"))
        .or_else(|| item.get("user_name"))
        .map(text)
        .filter(|s| !s.is_empty());
    let tags = item
        .get("tags")
        .and_then(|t| t.get("tags").or(Some(t)))
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(|t| {
                    t.as_str().map(String::from).or_else(|| {
                        t.get("tag")
                            .or_else(|| t.get("name"))
                            .and_then(Value::as_str)
                            .map(String::from)
                    })
                })
                .take(255)
                .collect()
        });
    Some(Manga {
        key: id.clone(),
        title: text(&item["title"]),
        cover: cover.and_then(Value::as_str).map(String::from),
        authors: author.map(|a| vec![a]),
        description: None,
        url: Some(format!("https://www.pixiv.net/artworks/{id}")),
        tags,
        status: MangaStatus::Completed,
        content_rating: if is_adult(item) {
            ContentRating::NSFW
        } else {
            ContentRating::Safe
        },
        viewer: Viewer::RightToLeft,
        ..Default::default()
    })
}

pub fn list(items: &[Value], has_next_page: bool, hide_adult: bool) -> MangaPageResult {
    MangaPageResult {
        entries: items
            .iter()
            .filter(|item| !hide_adult || !is_adult(item))
            .filter_map(manga)
            .collect(),
        // Pagination follows the upstream page, even when local filtering removes every item.
        has_next_page,
    }
}

pub fn chapter(item: &Value, key: &str) -> Chapter {
    let count = item
        .get("page_count")
        .or_else(|| item.get("pageCount"))
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let date = item
        .get("create_date")
        .or_else(|| item.get("createDate"))
        .and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp());
    Chapter {
        key: key.into(),
        title: Some(format!("全部图片（{count} 页）")),
        chapter_number: Some(1.0),
        date_uploaded: date,
        url: Some(format!("https://www.pixiv.net/artworks/{key}")),
        ..Default::default()
    }
}

pub fn image_pages(data: &Value, app: bool, original: bool) -> Result<Vec<Page>> {
    let mut images: Vec<String> = Vec::new();
    if app {
        if is_animation(data) {
            bail!("暂不支持 Pixiv 动图（ugoira）。");
        }
        if let Some(pages) = data
            .get("meta_pages")
            .and_then(Value::as_array)
            .filter(|p| !p.is_empty())
        {
            for page in pages {
                images.push(image_url(&page["image_urls"], original)?);
            }
        } else {
            let single = data
                .pointer("/meta_single_page/original_image_url")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            if original {
                images.push(
                    single
                        .map(String::from)
                        .map(Ok)
                        .unwrap_or_else(|| image_url(&data["image_urls"], true))?,
                );
            } else {
                images.push(image_url(&data["image_urls"], false)?);
            }
        }
    } else {
        let pages = data
            .as_array()
            .ok_or_else(|| error!("Pixiv 图片列表格式发生变化。"))?;
        for page in pages {
            images.push(image_url(&page["urls"], original)?);
        }
    }
    if images.is_empty() {
        bail!("没有可读取的图片，作品可能已删除或受到访问限制。");
    }
    Ok(images
        .into_iter()
        .map(|url| Page {
            content: PageContent::url(url),
            ..Default::default()
        })
        .collect())
}

fn image_url(urls: &Value, original: bool) -> Result<String> {
    let order = if original {
        ["original", "large", "regular", "medium"]
    } else {
        ["large", "regular", "medium", "original"]
    };
    order
        .iter()
        .find_map(|key| {
            urls.get(*key)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        })
        .map(String::from)
        .ok_or_else(|| error!("Pixiv 未返回图片地址，请检查账号访问权限。"))
}
