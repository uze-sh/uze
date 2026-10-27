//! `fetch(url, options)`: a request to a host a `net:` grant names
//! (`extension-growth` G1, G2).
//!
//! What the script asked is read and checked here, before the host sees
//! it: `https` only, a host the grants name, no credentials or port in the
//! URL, headers that stay one line each. The host makes the request; a
//! secret the script names is only a preference's name, whose value the
//! host places in the request and the script never holds.

use rhai::{Dynamic, Map};

use crate::reach::{Grants, ReachError};

/// What a script asked `fetch` for, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchRequest {
    /// The whole URL, as asked: `https://<host>[/path][?query]`.
    pub url: String,
    /// Its host, which a `net:` grant names.
    pub host: String,
    pub method: FetchMethod,
    /// Each header as `Name: value`, in the order asked.
    pub headers: Vec<(String, String)>,
    /// What a `POST` sends: text as given, a map as JSON.
    pub body: Option<String>,
    /// The secret preference placed in a header, when one is asked for.
    pub secret: Option<SecretUse>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchMethod {
    Get,
    Post,
}

impl FetchMethod {
    pub fn name(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

/// A secret preference, by name, and the header its value goes in:
/// `<header>: [<scheme> ]<value>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretUse {
    pub preference: String,
    pub header: String,
    pub scheme: Option<String>,
}

/// What the host answered a request with: the status and the body, the
/// body already scrubbed of any secret it carried.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FetchReply {
    pub status: u16,
    pub body: String,
}

/// The headers a script may not set: what the transport decides, and the
/// one a secret goes in when it asks for one.
const RESERVED_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "proxy-authorization",
];

/// The options `fetch` reads; anything else is refused, so a misspelt
/// `secret` never sends a request without it.
const OPTIONS: &[&str] = &["method", "headers", "body", "secret", "header", "scheme"];

impl FetchRequest {
    /// `url` and `options` as a request a host in `grants` answers, or why
    /// nothing is requested.
    pub fn read(url: &str, options: &Map, grants: &Grants) -> Result<Self, ReachError> {
        let host = granted_host(url, grants)?;
        if let Some(unknown) = options.keys().find(|key| !OPTIONS.contains(&key.as_str())) {
            return Err(refused(format!(
                "`{unknown}` is not an option of `fetch`; it reads {}",
                OPTIONS.join(", ")
            )));
        }
        let text = |key: &str| -> Result<Option<String>, ReachError> {
            match options.get(key) {
                None => Ok(None),
                Some(value) if value.is_unit() => Ok(None),
                Some(value) => value
                    .clone()
                    .into_string()
                    .map(Some)
                    .map_err(|kind| refused(format!("`{key}` is a `{kind}`, not a string"))),
            }
        };
        let method = match text("method")?.as_deref().map(str::to_ascii_uppercase) {
            None => FetchMethod::Get,
            Some(method) if method == "GET" => FetchMethod::Get,
            Some(method) if method == "POST" => FetchMethod::Post,
            Some(other) => {
                return Err(refused(format!(
                    "`{other}` is not a method `fetch` sends; it sends GET or POST"
                )));
            }
        };
        let body = match options.get("body") {
            None => None,
            Some(body) if body.is_unit() => None,
            Some(body) if body.is_string() => Some(body.clone().into_string().unwrap_or_default()),
            Some(body) => Some(
                crate::value::to_json(body)
                    .map_err(|crossing| refused(format!("the body is {crossing}")))?
                    .to_string(),
            ),
        };
        if body.is_some() && method == FetchMethod::Get {
            return Err(refused("a body is sent with `method: \"POST\"`".to_owned()));
        }
        let secret = match text("secret")? {
            None => {
                if options.contains_key("header") || options.contains_key("scheme") {
                    return Err(refused(
                        "`header` and `scheme` say where a `secret` goes, and no `secret` is named"
                            .to_owned(),
                    ));
                }
                None
            }
            Some(preference) => {
                let header = text("header")?.unwrap_or_else(|| "Authorization".to_owned());
                if !is_header_name(&header) {
                    return Err(refused(format!("`{header}` is not a header's name")));
                }
                let scheme = text("scheme")?;
                if let Some(scheme) = &scheme
                    && (scheme.is_empty()
                        || !scheme
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'))
                {
                    return Err(refused(format!(
                        "`{scheme}` is not a scheme such as `Bearer`"
                    )));
                }
                Some(SecretUse {
                    preference,
                    header,
                    scheme,
                })
            }
        };
        let headers = match options.get("headers") {
            None => Vec::new(),
            Some(headers) => headers_of(headers, secret.as_ref())?,
        };
        Ok(Self {
            url: url.to_owned(),
            host,
            method,
            headers,
            body,
            secret,
        })
    }
}

/// The host of `url` when it is an `https` URL of a host `grants` name.
pub fn granted_host(url: &str, grants: &Grants) -> Result<String, ReachError> {
    let host = host_of(url)?;
    if grants
        .iter()
        .filter_map(|grant| grant.host())
        .any(|granted| granted == host)
    {
        Ok(host)
    } else {
        Err(refused(format!(
            "`{host}` is a host this extension may not reach; it would need `net:{host}`"
        )))
    }
}

/// The host of an `https` URL, lower-cased, refusing what would make the
/// host read at consent differ from the one reached: credentials, a port
/// other than 443, a character the transport reads specially.
pub fn host_of(url: &str) -> Result<String, ReachError> {
    let Some(rest) = url.strip_prefix("https://") else {
        return Err(refused(format!("`{url}` is not an `https://` URL")));
    };
    if url.len() > 8192 {
        return Err(refused("the URL is longer than 8 KiB".to_owned()));
    }
    if url.chars().any(|ch| {
        ch.is_control() || ch.is_whitespace() || matches!(ch, '\\' | '{' | '}' | '[' | ']')
    }) {
        return Err(refused(format!(
            "`{url}` holds a character a URL is not written with; escape it"
        )));
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return Err(refused(
            "a URL does not carry credentials; name a `secret`".to_owned(),
        ));
    }
    let host = match authority.rsplit_once(':') {
        Some((host, "443")) => host,
        Some(_) => {
            return Err(refused(format!(
                "`{authority}` names a port; `fetch` reaches a host on 443 only"
            )));
        }
        None => authority,
    };
    let host = host.to_ascii_lowercase();
    if !uze_contract::is_host_name(&host) {
        return Err(refused(format!("`{host}` is not a host name")));
    }
    Ok(host)
}

/// `headers` as `(name, value)` pairs: a map of strings, each one line,
/// none the transport decides or the secret's own.
fn headers_of(
    headers: &Dynamic,
    secret: Option<&SecretUse>,
) -> Result<Vec<(String, String)>, ReachError> {
    let Some(map) = headers.read_lock::<Map>() else {
        return Err(refused("`headers` is a map of names to strings".to_owned()));
    };
    let mut pairs = Vec::with_capacity(map.len());
    for (name, value) in map.iter() {
        let name = name.to_string();
        if !is_header_name(&name) {
            return Err(refused(format!("`{name}` is not a header's name")));
        }
        let lower = name.to_ascii_lowercase();
        if RESERVED_HEADERS.contains(&lower.as_str()) {
            return Err(refused(format!(
                "`{name}` is set by the host, not a script"
            )));
        }
        if secret.is_some_and(|secret| secret.header.eq_ignore_ascii_case(&name)) {
            return Err(refused(format!(
                "`{name}` carries the secret, and is not set twice"
            )));
        }
        let value = value
            .clone()
            .into_string()
            .map_err(|kind| refused(format!("the header `{name}` is a `{kind}`, not a string")))?;
        if value.chars().any(char::is_control) {
            return Err(refused(format!(
                "the header `{name}` is more than one line"
            )));
        }
        pairs.push((name, value));
    }
    Ok(pairs)
}

/// A header's name as HTTP spells one: a token.
fn is_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

fn refused(reason: String) -> ReachError {
    ReachError(format!("fetch: {reason}"))
}

/// The reply as the script reads it: `#{status, body, json?}`, `json` only
/// when the body is JSON.
pub(crate) fn reply_map(reply: FetchReply) -> Map {
    let mut map = Map::new();
    map.insert(
        "status".into(),
        Dynamic::from(rhai::INT::from(reply.status)),
    );
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&reply.body)
        && let Ok(json) = crate::value::to_dynamic(&json)
    {
        map.insert("json".into(), json);
    }
    map.insert("body".into(), Dynamic::from(reply.body));
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reach::Grant;

    fn github() -> Grants {
        Grants::from([Grant::Net("api.github.com".into())])
    }

    fn read(url: &str, options: Map) -> Result<FetchRequest, ReachError> {
        FetchRequest::read(url, &options, &github())
    }

    fn options(pairs: &[(&str, Dynamic)]) -> Map {
        pairs
            .iter()
            .map(|(key, value)| ((*key).into(), value.clone()))
            .collect()
    }

    #[test]
    fn only_https_to_a_granted_host_is_asked() {
        let asked = read(
            "https://API.github.com/repos/a/b/actions/runs?branch=main",
            Map::new(),
        )
        .unwrap();
        assert_eq!(asked.host, "api.github.com");
        assert_eq!(asked.method, FetchMethod::Get);
        for (url, says) in [
            ("http://api.github.com/", "not an `https://` URL"),
            (
                "https://example.com/",
                "`example.com` is a host this extension may not reach",
            ),
            ("https://api.github.com.evil.io/", "may not reach"),
            (
                "https://user:pw@api.github.com/",
                "does not carry credentials",
            ),
            (
                "https://evil.io@api.github.com/",
                "does not carry credentials",
            ),
            ("https://api.github.com:8443/", "names a port"),
            (
                "https://api.github.com/{a,b}",
                "a character a URL is not written with",
            ),
            (
                "https://api.github.com/a b",
                "a character a URL is not written with",
            ),
            ("file:///etc/passwd", "not an `https://` URL"),
        ] {
            let refused = read(url, Map::new()).unwrap_err();
            assert!(refused.0.contains(says), "{url}: {refused}");
        }
        assert!(read("https://api.github.com:443/x", Map::new()).is_ok());
    }

    #[test]
    fn a_secret_is_named_never_given() {
        let asked = read(
            "https://api.github.com/user",
            options(&[("secret", "token".into()), ("scheme", "Bearer".into())]),
        )
        .unwrap();
        assert_eq!(
            asked.secret,
            Some(SecretUse {
                preference: "token".into(),
                header: "Authorization".into(),
                scheme: Some("Bearer".into()),
            })
        );
        let mut headers = Map::new();
        headers.insert("authorization".into(), "Bearer mine".into());
        let twice = read(
            "https://api.github.com/user",
            options(&[
                ("secret", "token".into()),
                ("headers", Dynamic::from_map(headers)),
            ]),
        )
        .unwrap_err();
        assert!(twice.0.contains("carries the secret"), "{twice}");
        assert!(
            read(
                "https://api.github.com/",
                options(&[("scheme", "Bearer".into())])
            )
            .is_err()
        );
        assert!(
            read(
                "https://api.github.com/",
                options(&[("secrets", "token".into())])
            )
            .is_err()
        );
    }

    #[test]
    fn a_header_is_one_line_the_host_does_not_decide() {
        let header = |name: &str, value: &str| {
            let mut headers = Map::new();
            headers.insert(name.into(), value.into());
            read(
                "https://api.github.com/",
                options(&[("headers", Dynamic::from_map(headers))]),
            )
        };
        assert!(header("Accept", "application/vnd.github+json").is_ok());
        assert!(header("Accept", "a\r\nHost: evil.io").is_err());
        assert!(header("Host", "evil.io").is_err());
        assert!(header("Bad Name", "x").is_err());
    }

    #[test]
    fn a_body_goes_with_post_and_a_map_as_json() {
        let mut body = Map::new();
        body.insert("ref".into(), "main".into());
        let asked = read(
            "https://api.github.com/x",
            options(&[("method", "post".into()), ("body", Dynamic::from_map(body))]),
        )
        .unwrap();
        assert_eq!(asked.method, FetchMethod::Post);
        assert_eq!(asked.body.as_deref(), Some(r#"{"ref":"main"}"#));
        assert!(read("https://api.github.com/x", options(&[("body", "x".into())])).is_err());
        assert!(
            read(
                "https://api.github.com/x",
                options(&[("method", "DELETE".into())])
            )
            .is_err()
        );
    }

    #[test]
    fn a_reply_carries_json_when_its_body_is() {
        let map = reply_map(FetchReply {
            status: 200,
            body: r#"{"total_count":2}"#.into(),
        });
        assert_eq!(map["status"].as_int().unwrap(), 200);
        assert!(map.contains_key("json"));
        let plain = reply_map(FetchReply {
            status: 404,
            body: "Not Found".into(),
        });
        assert!(!plain.contains_key("json"));
    }
}
