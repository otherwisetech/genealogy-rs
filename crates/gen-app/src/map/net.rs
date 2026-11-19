//! Blocking HTTPS for the map (run it on the background executor). One shared agent with an
//! identifying User-Agent, as the OSM tile and Nominatim usage policies require.
use std::sync::OnceLock;
use std::time::Duration;

pub fn user_agent() -> String {
    format!("genealogy-rs/{} (+https://github.com/otherwisetech/genealogy-rs)", env!("CARGO_PKG_VERSION")) // i18n-ok: HTTP header
}

pub fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| ureq::Agent::config_builder().user_agent(user_agent()).timeout_global(Some(Duration::from_secs(20))).max_redirects(3).build().into())
}

#[derive(Debug)]
pub enum NetError {
    /// HTTP status ≥ 400
    Status(u16),
    /// DNS, TLS, timeout, connection…
    Transport(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Status(c) => write!(f, "HTTP {c}"), // i18n-ok: log only
            NetError::Transport(e) => f.write_str(e),
        }
    }
}

impl From<ureq::Error> for NetError {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::StatusCode(c) => NetError::Status(c),
            e => NetError::Transport(e.to_string()),
        }
    }
}

pub fn get_bytes(url: &str, limit: u64) -> Result<Vec<u8>, NetError> {
    let mut resp = agent().get(url).call()?;
    Ok(resp.body_mut().with_config().limit(limit).read_to_vec()?)
}

pub fn get_text(url: &str, accept_language: &str) -> Result<String, NetError> {
    let mut resp = agent().get(url).header("Accept-Language", accept_language).call()?;
    Ok(resp.body_mut().with_config().limit(1 << 20).read_to_string()?)
}

/// Percent-encode a query value (RFC 3986 unreserved kept, space → %20).
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_query() {
        assert_eq!(super::encode("Villa Ñ, Río-1"), "Villa%20%C3%91%2C%20R%C3%ADo-1");
        assert_eq!(super::encode("a&b=c/d"), "a%26b%3Dc%2Fd");
        assert!(super::user_agent().starts_with("genealogy-rs/") && !super::user_agent().contains('@'));
    }
}
