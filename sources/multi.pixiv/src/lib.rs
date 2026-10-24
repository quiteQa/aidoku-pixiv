#![cfg_attr(not(test), no_std)]
#![allow(static_mut_refs)]
#![cfg_attr(test, allow(dead_code))]

mod auth;
mod models;

use aidoku::{
    alloc::{vec, String, Vec},
    imports::{
        defaults::{defaults_get, defaults_get_map, defaults_set, DefaultValue},
        html::Html,
        net::{set_rate_limit, Request, TimeUnit},
    },
    prelude::*,
    Chapter, DeepLinkHandler, DeepLinkResult, DynamicSettings, FilterValue, GroupSetting, HashMap,
    ImageRequestProvider, Listing, ListingProvider, Manga, MangaPageResult, NotificationHandler,
    Page, PageContext, Result, Setting, Source, WebLoginHandler,
};
use serde_json::Value;

struct Pixiv;

fn hide_adult() -> bool {
    defaults_get::<bool>("hideAdult").unwrap_or(true)
}

impl Pixiv {
    fn ranking(&self, id: &str, page: i32) -> Result<MangaPageResult> {
        if page < 1 {
            bail!("页码必须大于零。");
        }
        let (web_mode, app_mode) = match id {
            "daily" => ("daily", "day"),
            "weekly" => ("weekly", "week"),
            "monthly" => ("monthly", "month"),
            "manga" => ("daily", "day_manga"),
            _ => bail!("不支持的排行榜。"),
        };
        if auth::token_mode() {
            let data = auth::app_get(&format!(
                "/v1/illust/ranking?mode={app_mode}&filter=for_ios&offset={}",
                (i64::from(page) - 1) * 30
            ))?;
            let items = data["illusts"]
                .as_array()
                .ok_or_else(|| error!("Pixiv 排行榜格式发生变化。"))?;
            Ok(models::list(
                items,
                data["next_url"].as_str().is_some_and(|s| !s.is_empty()),
                hide_adult(),
            ))
        } else {
            // The web ranking endpoint provides at most ten pages of 50 entries.
            if page > 10 {
                return Ok(MangaPageResult::default());
            }
            let content = if id == "manga" { "&content=manga" } else { "" };
            let data = auth::web_get(&format!(
                "/ranking.php?format=json&mode={web_mode}&p={page}{content}"
            ))?;
            let items = data["contents"]
                .as_array()
                .ok_or_else(|| error!("Pixiv 排行榜格式发生变化。"))?;
            let next = data["next"].as_u64().is_some_and(|n| n > 0)
                || data["next"]
                    .as_str()
                    .is_some_and(|s| s.parse::<u64>().is_ok_and(|n| n > 0));
            Ok(models::list(items, next && page < 10, hide_adult()))
        }
    }

    fn detail(&self, id: &str) -> Result<Value> {
        if !models::valid_id(id) {
            bail!("无效的 Pixiv 作品 ID。");
        }
        let data = if auth::token_mode() {
            auth::app_get(&format!("/v1/illust/detail?illust_id={id}"))?["illust"].clone()
        } else {
            auth::web_get(&format!("/ajax/illust/{id}?lang=zh"))?
        };
        if models::is_animation(&data) {
            bail!("暂不支持 Pixiv 动图（ugoira）。");
        }
        if hide_adult() && models::is_adult(&data) {
            bail!("该作品已被“隐藏成人作品”设置过滤。");
        }
        if models::manga(&data).is_none() {
            bail!("Pixiv 未返回作品信息，作品可能已删除。");
        }
        Ok(data)
    }

    fn validate(&self) -> Result<()> {
        if auth::token_mode() {
            let data = auth::app_get("/v1/illust/follow?restrict=public&offset=0")?;
            if !data["illusts"].is_array() {
                bail!("Pixiv 未返回有效的登录状态。");
            }
        } else {
            if auth::setting("webLogin", "").is_empty()
                || !auth::validate_web(&defaults_get_map("webLogin").unwrap_or_default())?
            {
                bail!("网页登录已失效，请退出后重新登录。");
            }
        }
        Ok(())
    }
}

impl Source for Pixiv {
    fn new() -> Self {
        set_rate_limit(2, 1, TimeUnit::Seconds);
        Self
    }

    fn get_search_manga_list(
        &self,
        query: Option<String>,
        page: i32,
        filters: Vec<FilterValue>,
    ) -> Result<MangaPageResult> {
        if page < 1 {
            bail!("页码必须大于零。");
        }
        let mut query: String = query.unwrap_or_default().trim().into();
        let mut target = "partial";
        let mut ascending = false;
        for filter in &filters {
            match filter {
                FilterValue::Select { id, value } if id == "match" => target = value,
                FilterValue::Sort {
                    id,
                    ascending: value,
                    ..
                } if id == "sort" => ascending = *value,
                FilterValue::Text { id, value } if id == "tag" && query.is_empty() => {
                    query = value.clone()
                }
                _ => {}
            }
        }
        if query.is_empty() {
            return self.ranking("daily", page);
        }
        if let Some(id) =
            models::artwork_id(&query).or_else(|| models::valid_id(&query).then(|| query.clone()))
        {
            if page > 1 {
                return Ok(MangaPageResult::default());
            }
            return Ok(models::list(&[self.detail(&id)?], false, hide_adult()));
        }
        let word = auth::encode(&query);
        if auth::token_mode() {
            let target = match target {
                "exact" => "exact_match_for_tags",
                "text" => "title_and_caption",
                _ => "partial_match_for_tags",
            };
            let sort = if ascending { "date_asc" } else { "date_desc" };
            let data = auth::app_get(&format!("/v1/search/illust?word={word}&search_target={target}&sort={sort}&filter=for_ios&offset={}", (i64::from(page) - 1) * 30))?;
            let items = data["illusts"]
                .as_array()
                .ok_or_else(|| error!("Pixiv 搜索响应格式发生变化。"))?;
            Ok(models::list(
                items,
                data["next_url"].as_str().is_some_and(|s| !s.is_empty()),
                hide_adult(),
            ))
        } else {
            let mode = if hide_adult() { "safe" } else { "all" };
            let target = match target {
                "exact" => "s_tag_full",
                "text" => "s_tc",
                _ => "s_tag",
            };
            let order = if ascending { "date" } else { "date_d" };
            let data = auth::web_get(&format!("/ajax/search/artworks/{word}?word={word}&p={page}&order={order}&s_mode={target}&mode={mode}&type=all&lang=zh"))?;
            let result = &data["illustManga"];
            let items = result["data"]
                .as_array()
                .ok_or_else(|| error!("Pixiv 搜索响应格式发生变化。"))?;
            // Web search has 60 entries per page; filtering must not affect pagination.
            let total = result["total"]
                .as_u64()
                .ok_or_else(|| error!("Pixiv 搜索响应缺少总数。"))?;
            Ok(models::list(
                items,
                (page as u64) * 60 < total && page < 1000,
                hide_adult(),
            ))
        }
    }

    fn get_manga_update(
        &self,
        mut manga: Manga,
        needs_details: bool,
        needs_chapters: bool,
    ) -> Result<Manga> {
        if !needs_details && !needs_chapters {
            return Ok(manga);
        }
        let data = self.detail(&manga.key)?;
        if needs_details {
            let mut updated = models::manga(&data).ok_or_else(|| error!("无法读取作品详情。"))?;
            let caption = data
                .get("caption")
                .or_else(|| data.get("description"))
                .and_then(Value::as_str)
                .unwrap_or("");
            updated.description = if caption.is_empty() {
                None
            } else {
                Html::parse(
                    caption
                        .replace("<br />", "\n")
                        .replace("<br>", "\n")
                        .replace("<br/>", "\n"),
                )
                .ok()
                .and_then(|d| d.select_first("body").and_then(|e| e.text()))
            };
            manga.copy_from(updated);
        }
        if needs_chapters {
            manga.chapters = Some(vec![models::chapter(&data, &manga.key)]);
        }
        Ok(manga)
    }

    fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
        if chapter.key != manga.key {
            bail!("章节与 Pixiv 作品不匹配。");
        }
        let original = defaults_get::<bool>("originalImages").unwrap_or(true);
        let detail = self.detail(&manga.key)?;
        if auth::token_mode() {
            models::image_pages(&detail, true, original)
        } else {
            models::image_pages(
                &auth::web_get(&format!("/ajax/illust/{}/pages?lang=zh", manga.key))?,
                false,
                original,
            )
        }
    }
}

impl ListingProvider for Pixiv {
    fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
        self.ranking(&listing.id, page)
    }
}

impl ImageRequestProvider for Pixiv {
    fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
        // Never attach account tokens or session cookies to image URLs.
        Ok(Request::get(url)?
            .header("Referer", "https://www.pixiv.net/")
            .timeout(30.0))
    }
}

impl WebLoginHandler for Pixiv {
    fn handle_web_login(&self, key: String, cookies: HashMap<String, String>) -> Result<bool> {
        if key != "webLogin" {
            return Ok(false);
        }
        if auth::validate_web(&cookies)? {
            auth::set_string("authMode", "web");
            auth::set_string("loginStatus", "网页登录成功");
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl NotificationHandler for Pixiv {
    fn handle_notification(&self, notification: String) {
        match notification.as_str() {
            "tokenChanged" => {
                auth::clear_cache();
                auth::set_string("loginStatus", "Token 已修改，请选择 Token 登录后点击验证。");
            }
            "clearToken" => {
                auth::clear_cache();
                defaults_set("tokenInput", DefaultValue::Null);
                auth::set_string("loginStatus", "Token 已清除。");
            }
            "authChanged" | "webLoginChanged" => {
                auth::set_string("loginStatus", "登录方式或会话已变更，请验证当前登录。")
            }
            "validateLogin" => {
                let status = match self.validate() {
                    Ok(()) => String::from("验证成功，可以浏览和阅读作品。"),
                    Err(aidoku::AidokuError::Message(message)) => message,
                    Err(_) => String::from("验证失败，请检查网络后重试。"),
                };
                auth::set_string("loginStatus", status);
            }
            _ => {}
        }
    }
}

impl DynamicSettings for Pixiv {
    fn get_dynamic_settings(&self) -> Result<Vec<Setting>> {
        let mode = if auth::token_mode() {
            "Token 登录"
        } else {
            "网页登录"
        };
        let status = auth::setting("loginStatus", "尚未验证登录。");
        Ok(vec![GroupSetting {
            key: "loginState".into(),
            title: "登录状态".into(),
            footer: Some(format!("当前方式：{mode}\n{status}").into()),
            items: Vec::new(),
            ..Default::default()
        }
        .into()])
    }
}

impl DeepLinkHandler for Pixiv {
    fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
        Ok(models::artwork_id(&url).map(|key| DeepLinkResult::Manga { key }))
    }
}

#[cfg(target_family = "wasm")]
register_source!(
    Pixiv,
    ListingProvider,
    ImageRequestProvider,
    WebLoginHandler,
    NotificationHandler,
    DynamicSettings,
    DeepLinkHandler
);

#[cfg(test)]
mod tests;
