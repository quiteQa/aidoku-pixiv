use super::*;
use aidoku::{ContentRating, PageContent};
use serde_json::json;

// Page's drop glue references this WASM host function even for URL-only pages.
// Pure model tests must never actually invoke the host.
#[no_mangle]
extern "C" fn destroy(_rid: i32) {
    panic!("native model tests must not call Aidoku host imports");
}

#[test]
fn token_normalization_and_query_encoding() {
    assert_eq!(
        auth::clean_token("  Bearer abc.def-123  ").unwrap(),
        "abc.def-123"
    );
    assert!(auth::clean_token(" \n").is_err());
    assert!(auth::clean_token("abc\r\nInjected:header").is_err());
    assert_eq!(auth::encode("猫 &+/#"), "%E7%8C%AB%20%26%2B%2F%23");
}

#[test]
fn web_login_requires_real_session_cookie() {
    let mut cookies = HashMap::new();
    cookies.insert("device_token".into(), "anonymous".into());
    assert!(auth::session(&cookies).is_none());
    cookies.insert("PHPSESSID".into(), "123_abcdef".into());
    assert_eq!(auth::session(&cookies), Some("123_abcdef"));
    cookies.insert("PHPSESSID".into(), "abc; malicious=value".into());
    assert!(auth::session(&cookies).is_none());
}

#[test]
fn urls_and_ids_are_validated() {
    for url in [
        "https://www.pixiv.net/artworks/123",
        "https://www.pixiv.net/en/artworks/123?x=1#p",
        "https://www.pixiv.net/member_illust.php?mode=medium&illust_id=123",
    ] {
        assert_eq!(models::artwork_id(url).as_deref(), Some("123"));
    }
    for url in [
        "https://evil.test/artworks/123",
        "https://www.pixiv.net.evil.test/artworks/123",
        "https://www.pixiv.net/artworks/123/other",
        "https://www.pixiv.net/users/123",
    ] {
        assert!(models::artwork_id(url).is_none());
    }
    assert!(!models::valid_id("../123"));
    assert!(!models::valid_id("0"));
}

#[test]
fn parses_app_web_and_ranking_artwork_shapes() {
    let app = json!({"id": 123, "title": "App", "user": {"name": "作者"}, "image_urls": {"large": "https://i.pximg.net/app.jpg"}, "tags": [{"name": "猫"}], "x_restrict": 1});
    let web = json!({"id": "123", "title": "Web", "userName": "作者", "urls": {"regular": "https://i.pximg.net/web.jpg"}, "tags": {"tags": [{"tag": "猫"}]}, "xRestrict": 0});
    let rank = json!({"illust_id": 123, "title": "Rank", "user_name": "作者", "url": "https://i.pximg.net/rank.jpg", "tags": ["猫"], "illust_type": "1"});
    for item in [app, web, rank] {
        let manga = models::manga(&item).unwrap();
        assert_eq!(manga.key, "123");
        assert_eq!(manga.authors.unwrap()[0], "作者");
        assert_eq!(manga.tags.unwrap()[0], "猫");
        assert!(manga.cover.unwrap().starts_with("https://i.pximg.net/"));
    }
}

#[test]
fn filters_adult_animation_and_ads_without_ending_pagination() {
    let items = vec![
        json!({"id": "1", "illustType": 2}),
        json!({"id": "2", "type": "ugoira"}),
        json!({"id": "3", "illust_type": "2"}),
        json!({"id": "4", "xRestrict": 1}),
        json!({"id": "5", "isAdContainer": true}),
    ];
    let filtered = models::list(&items, true, models::ContentFilter::default());
    assert!(filtered.entries.is_empty());
    assert!(filtered.has_next_page);
    let visible = models::list(
        &items,
        false,
        models::ContentFilter {
            show_ai: true,
            show_adult: true,
        },
    );
    assert_eq!(visible.entries.len(), 1);
    assert_eq!(visible.entries[0].content_rating, ContentRating::NSFW);
}

#[test]
fn content_filters_are_independent_and_accept_web_app_markers() {
    let items = vec![
        json!({"id":"1", "illust_ai_type":1, "x_restrict":0}),
        json!({"id":"2", "illust_ai_type":2, "x_restrict":0}),
        json!({"id":"3", "aiType":1, "xRestrict":1}),
        json!({"id":"4", "aiType":2, "xRestrict":2}),
        json!({"id":"5", "ai_type":"2", "xRestrict":"0"}),
    ];
    for (show_ai, show_adult, expected) in [
        (false, false, vec!["1"]),
        (true, false, vec!["1", "2", "5"]),
        (false, true, vec!["1", "3"]),
        (true, true, vec!["1", "2", "3", "4", "5"]),
    ] {
        let page = models::list(
            &items,
            true,
            models::ContentFilter {
                show_ai,
                show_adult,
            },
        );
        assert_eq!(
            page.entries
                .iter()
                .map(|m| m.key.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(page.has_next_page);
    }
    assert!(!models::is_adult(
        &json!({"xRestrict":0, "illust_content_type":{"sexual":1}})
    ));
    assert_eq!(models::ai_type(&json!({"illust_ai_type":0})), Some(0));
    assert_eq!(models::ai_type(&json!({})), None);
    assert!(models::ContentFilter::default().needs_metadata(&json!({"illust_id":1})));
}

#[test]
fn original_images_and_web_page_order() {
    let data = json!([
        {"urls": {"original": "https://i.pximg.net/0.png", "regular": "https://i.pximg.net/0.jpg"}},
        {"urls": {"original": "https://i.pximg.net/1.png", "regular": "https://i.pximg.net/1.jpg"}}
    ]);
    let original = models::image_pages(&data, false, true).unwrap();
    let regular = models::image_pages(&data, false, false).unwrap();
    assert_eq!(original.len(), 2);
    assert_eq!(
        original[1].content,
        PageContent::url("https://i.pximg.net/1.png")
    );
    assert_eq!(
        regular[0].content,
        PageContent::url("https://i.pximg.net/0.jpg")
    );
}

#[test]
fn app_single_and_multi_page_images() {
    let single = json!({"meta_single_page": {"original_image_url": "https://i.pximg.net/single.png"}, "image_urls": {"large": "https://i.pximg.net/single.jpg"}, "meta_pages": []});
    assert_eq!(
        models::image_pages(&single, true, true).unwrap()[0].content,
        PageContent::url("https://i.pximg.net/single.png")
    );
    assert_eq!(
        models::image_pages(&single, true, false).unwrap()[0].content,
        PageContent::url("https://i.pximg.net/single.jpg")
    );
    let multi = json!({"meta_pages": [
        {"image_urls": {"original": "https://i.pximg.net/0.png", "large": "https://i.pximg.net/0.jpg"}},
        {"image_urls": {"original": "https://i.pximg.net/1.png", "large": "https://i.pximg.net/1.jpg"}}
    ]});
    assert_eq!(models::image_pages(&multi, true, true).unwrap().len(), 2);
    assert!(models::image_pages(&json!([]), false, true).is_err());
    assert!(models::image_pages(&json!([{"urls": {}}]), false, true).is_err());
}

#[test]
fn chapter_has_stable_key_page_count_and_timestamp() {
    let chapter = models::chapter(
        &json!({"page_count": 3, "create_date": "2026-10-24T12:00:00+09:00"}),
        "123",
    );
    assert_eq!(chapter.key, "123");
    assert_eq!(chapter.title.as_deref(), Some("全部图片（3 页）"));
    assert!(chapter.date_uploaded.unwrap() > 1_700_000_000);
}
