use aidoku::{
    alloc::{string::ToString, String},
    imports::{
        defaults::{defaults_get, defaults_get_map, defaults_set, DefaultValue},
        net::{Request, Response},
        std::current_date,
    },
    prelude::*,
    HashMap, Result,
};
use chrono::{DateTime, Utc};
use md5::{Digest, Md5};
use serde_json::Value;

pub const WEB: &str = "https://www.pixiv.net";
pub const APP: &str = "https://app-api.pixiv.net";
const OAUTH: &str = "https://oauth.secure.pixiv.net/auth/token";
const APP_UA: &str = "PixivIOSApp/7.13.3 (iOS 14.6; iPhone13,2)";
// Public mobile client parameters used by Pixiv's app API (not user credentials).
const CLIENT_ID: &str = "MOBrBDS8blbauoSck0ZfDbtuzpyT";
const CLIENT_SECRET: &str = "lsACyCD94FhDUtGTXi3QzcFE2uU1hqtDaKeqrdwj";
const HASH_SALT: &str = "28c1fdd170a5204386cb1313c7077b34f83e4aaf4aa829ce78c231e05b0bae2c";
const CACHE_KEYS: &[&str] = &[
    "cachedAccess",
    "cachedRefresh",
    "tokenFingerprint",
    "expiresAt",
];

pub fn setting(key: &str, fallback: &str) -> String {
    defaults_get::<String>(key).unwrap_or_else(|| fallback.into())
}

pub fn set_string(key: &str, value: impl Into<String>) {
    defaults_set(key, DefaultValue::String(value.into()));
}

pub fn token_mode() -> bool {
    setting("authMode", "web") == "token"
}

pub fn clear_cache() {
    for key in CACHE_KEYS {
        defaults_set(key, DefaultValue::Null);
    }
}

pub fn encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

pub fn clean_token(value: &str) -> Result<String> {
    let value = value.trim();
    let value = if value
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("bearer "))
    {
        value[7..].trim()
    } else {
        value
    };
    if value.is_empty() {
        bail!("请在源设置中填写 Token，再验证登录。");
    }
    if value
        .bytes()
        .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
    {
        bail!("Token 中不能包含空白或换行，请重新粘贴完整 Token。");
    }
    Ok(value.into())
}

pub fn session(cookies: &HashMap<String, String>) -> Option<&str> {
    cookies.get("PHPSESSID").map(String::as_str).filter(|s| {
        !s.is_empty()
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-%.".contains(&c))
    })
}

fn browser_request(path: &str, session: &str) -> Result<Request> {
    Ok(Request::get(format!("{WEB}{path}"))?
        .header("Referer", "https://www.pixiv.net/")
        .header("User-Agent", "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Version/18.0 Mobile/15E148 Safari/604.1")
        .header("Accept-Language", "zh-CN,ja;q=0.9,en;q=0.8")
        .header("Cookie", format!("PHPSESSID={session}").as_str())
        .timeout(30.0))
}

pub fn validate_web(cookies: &HashMap<String, String>) -> Result<bool> {
    let Some(session) = session(cookies) else {
        return Ok(false);
    };
    let response = browser_request("/ajax/user/extra?lang=zh", session)?.send()?;
    if response.status_code() != 200 {
        return Ok(false);
    }
    let data: Value = response
        .get_json_owned()
        .map_err(|_| error!("Pixiv 登录验证返回了无法识别的响应，请重试。"))?;
    Ok(data.get("error").and_then(Value::as_bool) == Some(false)
        && data.get("body").is_some_and(Value::is_object))
}

pub fn web_get(path: &str) -> Result<Value> {
    if setting("webLogin", "").is_empty() {
        bail!("请先在源设置中选择网页登录，并完成 Pixiv 登录。");
    }
    let cookies = defaults_get_map("webLogin").unwrap_or_default();
    let session = session(&cookies).ok_or_else(|| error!("网页登录已失效，请退出后重新登录。"))?;
    let value = read_response(browser_request(path, session)?.send()?)?;
    if path.starts_with("/ajax/") {
        if value.get("error").and_then(Value::as_bool) != Some(false) {
            bail!("Pixiv 拒绝了请求；请检查登录状态及作品访问权限。");
        }
        value
            .get("body")
            .cloned()
            .ok_or_else(|| error!("Pixiv 响应缺少 body。"))
    } else {
        Ok(value)
    }
}

fn app_headers(request: Request) -> Request {
    request
        .header("User-Agent", APP_UA)
        .header("App-OS", "ios")
        .header("App-OS-Version", "14.6")
        .header("Accept-Language", "zh-CN")
        .timeout(30.0)
}

fn digest(value: &str) -> String {
    let mut output = String::new();
    for byte in Md5::digest(value.as_bytes()) {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn access_token(force_refresh: bool) -> Result<String> {
    let input = clean_token(&setting("tokenInput", ""))?;
    if setting("tokenKind", "refresh") == "access" {
        return Ok(input);
    }
    let fingerprint = digest(&input);
    let same_input = setting("tokenFingerprint", "") == fingerprint;
    let expires = setting("expiresAt", "0").parse::<i64>().unwrap_or(0);
    let cached = setting("cachedAccess", "");
    if same_input && !force_refresh && expires > current_date() + 60 && !cached.is_empty() {
        return Ok(cached);
    }
    let rotated = setting("cachedRefresh", "");
    let refresh = if same_input && !rotated.is_empty() {
        rotated
    } else {
        input
    };
    let time = DateTime::<Utc>::from_timestamp(current_date(), 0)
        .ok_or_else(|| error!("设备时间无效，请检查系统时钟。"))?
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string();
    let body = format!(
        "client_id={CLIENT_ID}&client_secret={CLIENT_SECRET}&grant_type=refresh_token&get_secure_url=1&refresh_token={}",
        encode(&refresh)
    );
    let response = app_headers(Request::post(OAUTH)?)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("X-Client-Time", &time)
        .header("X-Client-Hash", &digest(&format!("{time}{HASH_SALT}")))
        .body(body)
        .send()?;
    if response.status_code() == 400 || response.status_code() == 401 {
        clear_cache();
        bail!("Refresh Token 无效或已失效，请更换 Token 后重新验证登录。");
    }
    let value = read_response(response)?;
    // Pixiv has used both a top-level object and a nested `response` envelope.
    let token = value.get("response").unwrap_or(&value);
    let access = token
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| error!("Pixiv 未返回 Access Token。"))?;
    let new_refresh = token
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(&refresh);
    let duration = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3600)
        .clamp(0, 86400);
    set_string("cachedAccess", access);
    set_string("cachedRefresh", new_refresh);
    set_string("tokenFingerprint", fingerprint);
    set_string("expiresAt", (current_date() + duration).to_string());
    Ok(access.into())
}

pub fn app_get(path: &str) -> Result<Value> {
    for attempt in 0..2 {
        let token = access_token(attempt == 1)?;
        let response = app_headers(Request::get(format!("{APP}{path}"))?)
            .header("Authorization", format!("Bearer {token}").as_str())
            .send()?;
        let status = response.status_code();
        if status == 401 {
            if attempt == 0 && setting("tokenKind", "refresh") == "refresh" {
                continue;
            }
            bail!("Access Token 已失效，请更新 Token，或改用 Refresh Token 自动续期。");
        }
        // Some app API versions report an expired token as HTTP 400 + OAuth error.
        if status == 400 {
            let data: Value = response.get_json_owned().unwrap_or(Value::Null);
            let oauth_error = data
                .pointer("/error/message")
                .and_then(Value::as_str)
                .is_some_and(|s| s.contains("OAuth"))
                || data
                    .pointer("/error/reason")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.contains("token"));
            if oauth_error && attempt == 0 && setting("tokenKind", "refresh") == "refresh" {
                continue;
            }
            bail!("Pixiv 拒绝了请求，请检查 Token 和搜索条件。");
        }
        let value = read_response(response)?;
        if value
            .get("error")
            .is_some_and(|e| !e.is_null() && e != false)
        {
            bail!("Pixiv API 返回错误，请检查登录状态或作品访问权限。");
        }
        return Ok(value);
    }
    bail!("Token 自动续期失败，请重新登录。");
}

fn read_response(response: Response) -> Result<Value> {
    match response.status_code() {
        200..=299 => response
            .get_json_owned()
            .map_err(|_| error!("Pixiv 返回了无法识别的响应，请稍后重试。")),
        401 => Err(error!("登录已失效，请在源设置中重新登录。")),
        403 => Err(error!(
            "Pixiv 拒绝访问，请检查账号权限或在网页登录中完成验证。"
        )),
        404 => Err(error!("作品不存在、已删除，或当前账号无权访问。")),
        429 => Err(error!("请求过于频繁，请稍后再试。")),
        code => Err(error!("Pixiv 请求失败（HTTP {code}），请稍后重试。")),
    }
}
