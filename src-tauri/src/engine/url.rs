//! URL presentation, validation and deterministic primary-URL selection.

use crate::error::{AppError, AppResult};
use crate::models::{HttpState, PrimaryUrl, PrimaryUrlSource};
use std::net::IpAddr;

/// Browser-usable host for a listening address. Wildcard and loopback
/// addresses become `localhost` (what developers register for OAuth/CORS);
/// other specific addresses are kept as-is.
pub fn browser_host(address: IpAddr) -> String {
    if address.is_unspecified() || address.is_loopback() {
        return "localhost".to_string();
    }
    match address {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => format!("[{v6}]"),
    }
}

/// `http://host:port` for the best of an endpoint's addresses
/// (wildcard/loopback first, so `0.0.0.0` never becomes the browser host).
pub fn http_url(addresses: &[IpAddr], port: u16) -> String {
    let best = addresses
        .iter()
        .find(|a| a.is_unspecified() || a.is_loopback())
        .or_else(|| addresses.first());
    let host = best
        .map(|a| browser_host(*a))
        .unwrap_or_else(|| "localhost".to_string());
    format!("http://{host}:{port}")
}

/// Validates a user-supplied primary URL. Only `http`/`https` with a host are
/// accepted; a bare `host:port` (`localhost:3000`) gets `http://`.
pub fn validate_http_url(input: &str) -> AppResult<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("URL is empty".to_string()));
    }
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::InvalidInput(
            "URL must not contain whitespace".to_string(),
        ));
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else if looks_like_host_port(trimmed) {
        format!("http://{trimmed}")
    } else {
        return Err(AppError::InvalidInput(
            "URL must start with http:// or https://".to_string(),
        ));
    };
    let (scheme, rest) = candidate
        .split_once("://")
        .ok_or_else(|| AppError::InvalidInput("URL is malformed".to_string()))?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(AppError::InvalidInput(
            "Only http:// and https:// URLs can be opened".to_string(),
        ));
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host_part = authority.rsplit('@').next().unwrap_or(authority);
    if host_part.is_empty() || host_part.starts_with(':') {
        return Err(AppError::InvalidInput("URL has no host".to_string()));
    }
    Ok(format!("{scheme}://{rest}"))
}

fn looks_like_host_port(value: &str) -> bool {
    let Some((host, port)) = value.rsplit_once(':') else {
        return false;
    };
    let port = port.split('/').next().unwrap_or("");
    !host.is_empty() && !port.is_empty() && port.chars().all(|c| c.is_ascii_digit())
}

/// A project HTTP endpoint considered for the primary URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlCandidate {
    pub port: u16,
    pub url: String,
    pub http: HttpState,
    /// Owned by a Web/frontend component.
    pub web_component: bool,
}

/// Priority: 1) explicit override, 2) HTTP endpoint of a Web component,
/// 3) known dev-server endpoint, 4) first confirmed HTTP endpoint in stable
/// (port, url) order, 5) none.
pub fn select_primary_url(
    override_url: Option<&str>,
    candidates: &[UrlCandidate],
) -> Option<PrimaryUrl> {
    if let Some(url) = override_url.map(str::trim).filter(|u| !u.is_empty()) {
        return Some(PrimaryUrl {
            url: url.to_string(),
            source: PrimaryUrlSource::Override,
        });
    }
    let mut ordered: Vec<&UrlCandidate> = candidates
        .iter()
        .filter(|c| matches!(c.http, HttpState::Confirmed | HttpState::Likely))
        .collect();
    ordered.sort_by(|a, b| (a.port, &a.url).cmp(&(b.port, &b.url)));

    let pick = |source: PrimaryUrlSource, found: Option<&&UrlCandidate>| {
        found.map(|c| PrimaryUrl {
            url: c.url.clone(),
            source,
        })
    };
    pick(
        PrimaryUrlSource::WebComponent,
        ordered.iter().find(|c| c.web_component),
    )
    .or_else(|| {
        pick(
            PrimaryUrlSource::DevServer,
            ordered.iter().find(|c| c.http == HttpState::Likely),
        )
    })
    .or_else(|| {
        pick(
            PrimaryUrlSource::ConfirmedHttp,
            ordered.iter().find(|c| c.http == HttpState::Confirmed),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    fn cand(port: u16, http: HttpState, web: bool) -> UrlCandidate {
        UrlCandidate {
            port,
            url: format!("http://localhost:{port}"),
            http,
            web_component: web,
        }
    }

    #[test]
    fn wildcard_and_loopback_listeners_become_localhost() {
        assert_eq!(http_url(&[v4(0, 0, 0, 0)], 3000), "http://localhost:3000");
        assert_eq!(
            http_url(&[IpAddr::V6(Ipv6Addr::UNSPECIFIED)], 3000),
            "http://localhost:3000"
        );
        assert_eq!(http_url(&[v4(127, 0, 0, 1)], 5173), "http://localhost:5173");
        assert_eq!(
            http_url(&[IpAddr::V6(Ipv6Addr::LOCALHOST)], 5173),
            "http://localhost:5173"
        );
    }

    #[test]
    fn wildcard_wins_over_a_lan_address_on_the_same_endpoint() {
        assert_eq!(
            http_url(&[v4(192, 168, 1, 20), v4(0, 0, 0, 0)], 8080),
            "http://localhost:8080"
        );
    }

    #[test]
    fn specific_non_loopback_addresses_are_kept() {
        assert_eq!(
            http_url(&[v4(192, 168, 1, 20)], 8080),
            "http://192.168.1.20:8080"
        );
        let v6: IpAddr = "fe80::1".parse().unwrap();
        assert_eq!(http_url(&[v6], 8080), "http://[fe80::1]:8080");
    }

    #[test]
    fn validates_and_normalises_override_urls() {
        assert_eq!(
            validate_http_url(" http://localhost:3000 ").unwrap(),
            "http://localhost:3000"
        );
        assert_eq!(
            validate_http_url("HTTPS://Example.com/a?b=1").unwrap(),
            "https://Example.com/a?b=1"
        );
        assert_eq!(
            validate_http_url("localhost:3000").unwrap(),
            "http://localhost:3000"
        );
        assert_eq!(
            validate_http_url("myapp.test:8080/admin").unwrap(),
            "http://myapp.test:8080/admin"
        );
    }

    #[test]
    fn rejects_unsafe_or_malformed_override_urls() {
        for bad in [
            "",
            "   ",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://host/x",
            "mailto:a@b.c",
            "http://",
            "http:///path",
            "http://:3000",
            "http://exa mple.com",
            "just text",
            "tauri://localhost",
        ] {
            let err = validate_http_url(bad).unwrap_err();
            assert_eq!(err.code(), crate::error::ErrorCode::InvalidInput, "{bad:?}");
        }
    }

    #[test]
    fn override_always_wins() {
        let candidates = [cand(3000, HttpState::Confirmed, true)];
        let url = select_primary_url(Some("https://app.test"), &candidates).unwrap();
        assert_eq!(url.source, PrimaryUrlSource::Override);
        assert_eq!(url.url, "https://app.test");
        assert!(select_primary_url(Some("https://app.test"), &[]).is_some());
    }

    #[test]
    fn web_component_beats_dev_server_beats_confirmed() {
        let all = [
            cand(4000, HttpState::Confirmed, false),
            cand(3500, HttpState::Likely, false),
            cand(5173, HttpState::Likely, true),
        ];
        let chosen = select_primary_url(None, &all).unwrap();
        assert_eq!(
            (chosen.url.as_str(), chosen.source),
            ("http://localhost:5173", PrimaryUrlSource::WebComponent)
        );

        let chosen = select_primary_url(None, &all[..2]).unwrap();
        assert_eq!(
            (chosen.url.as_str(), chosen.source),
            ("http://localhost:3500", PrimaryUrlSource::DevServer)
        );

        let chosen = select_primary_url(None, &all[..1]).unwrap();
        assert_eq!(
            (chosen.url.as_str(), chosen.source),
            ("http://localhost:4000", PrimaryUrlSource::ConfirmedHttp)
        );
    }

    #[test]
    fn selection_is_deterministic_by_port_regardless_of_input_order() {
        let a = cand(9000, HttpState::Confirmed, false);
        let b = cand(8000, HttpState::Confirmed, false);
        let one = select_primary_url(None, &[a.clone(), b.clone()]).unwrap();
        let two = select_primary_url(None, &[b, a]).unwrap();
        assert_eq!(one, two);
        assert_eq!(one.url, "http://localhost:8000");
    }

    #[test]
    fn non_http_and_unconfirmed_ports_never_become_the_url() {
        let all = [
            cand(5432, HttpState::None, true),
            cand(6379, HttpState::Probing, true),
        ];
        assert_eq!(select_primary_url(None, &all), None);
    }

    #[test]
    fn blank_override_is_ignored() {
        let all = [cand(3000, HttpState::Likely, false)];
        let chosen = select_primary_url(Some("  "), &all).unwrap();
        assert_eq!(chosen.source, PrimaryUrlSource::DevServer);
    }
}
