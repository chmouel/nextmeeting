//! HTTP authentication utilities for CalDAV.
//!
//! This module implements HTTP Basic and Digest authentication as per:
//! - RFC 7617 (Basic)
//! - RFC 7616 (Digest)

use base64::Engine;
use rand::Rng;
use std::collections::HashMap;

/// HTTP Digest authentication handler.
///
/// Implements RFC 7616 Digest Access Authentication.
#[derive(Debug, Clone)]
pub struct DigestAuth {
    /// The realm from the server challenge.
    pub realm: String,
    /// The nonce from the server challenge.
    pub nonce: String,
    /// The opaque value from the server challenge (optional).
    pub opaque: Option<String>,
    /// The quality of protection (qop) options.
    pub qop: Option<String>,
    /// The algorithm (defaults to MD5).
    pub algorithm: String,
    /// Client nonce counter.
    nc: u32,
}

impl DigestAuth {
    /// Parses a WWW-Authenticate header to extract digest parameters.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let header = "Digest realm=\"example\", nonce=\"abc123\", qop=\"auth\"";
    /// let auth = DigestAuth::parse(header)?;
    /// ```
    pub fn parse(header: &str) -> Option<Self> {
        // Must start with "Digest "
        let content = header.strip_prefix("Digest ")?.trim();

        let params = parse_auth_params(content);

        let realm = params.get("realm")?.to_string();
        let nonce = params.get("nonce")?.to_string();
        let opaque = params.get("opaque").map(|s| s.to_string());
        let qop = params.get("qop").map(|s| s.to_string());
        let algorithm = params
            .get("algorithm")
            .map(|s| s.to_string())
            .unwrap_or_else(|| "MD5".to_string());

        Some(Self {
            realm,
            nonce,
            opaque,
            qop,
            algorithm,
            nc: 0,
        })
    }

    /// Generates an Authorization header value for a request.
    ///
    /// # Arguments
    ///
    /// * `method` - HTTP method (GET, PROPFIND, REPORT, etc.)
    /// * `uri` - Request URI path
    /// * `username` - Username for authentication
    /// * `password` - Password for authentication
    pub fn authorize(&mut self, method: &str, uri: &str, username: &str, password: &str) -> String {
        self.nc += 1;
        let nc = format!("{:08x}", self.nc);
        let cnonce = generate_cnonce();

        // HA1 = MD5(username:realm:password)
        let ha1 = md5_hex(&format!("{}:{}:{}", username, self.realm, password));

        // HA2 = MD5(method:uri)
        let ha2 = md5_hex(&format!("{}:{}", method, uri));

        // Calculate response based on qop
        let response = if let Some(ref qop) = self.qop {
            if qop.contains("auth") {
                // response = MD5(HA1:nonce:nc:cnonce:qop:HA2)
                md5_hex(&format!(
                    "{}:{}:{}:{}:auth:{}",
                    ha1, self.nonce, nc, cnonce, ha2
                ))
            } else {
                // No qop, simpler calculation
                md5_hex(&format!("{}:{}:{}", ha1, self.nonce, ha2))
            }
        } else {
            // RFC 2069 compatibility (no qop)
            md5_hex(&format!("{}:{}:{}", ha1, self.nonce, ha2))
        };

        // Build the Authorization header
        let mut parts = vec![
            format!("username=\"{}\"", username),
            format!("realm=\"{}\"", self.realm),
            format!("nonce=\"{}\"", self.nonce),
            format!("uri=\"{}\"", uri),
            format!("response=\"{}\"", response),
            format!("algorithm={}", self.algorithm),
        ];

        if self.qop.is_some() {
            parts.push(format!("qop=auth"));
            parts.push(format!("nc={}", nc));
            parts.push(format!("cnonce=\"{}\"", cnonce));
        }

        if let Some(ref opaque) = self.opaque {
            parts.push(format!("opaque=\"{}\"", opaque));
        }

        format!("Digest {}", parts.join(", "))
    }
}

/// Generates a Basic authentication header value.
pub fn basic_auth(username: &str, password: &str) -> String {
    let credentials = format!("{}:{}", username, password);
    let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
    format!("Basic {}", encoded)
}

/// Parses authentication parameters from a WWW-Authenticate header value.
fn parse_auth_params(content: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    let mut chars = content.chars().peekable();

    while chars.peek().is_some() {
        // Skip whitespace and commas
        while chars.peek().is_some_and(|c| c.is_whitespace() || *c == ',') {
            chars.next();
        }

        // Read key
        let key: String = chars
            .by_ref()
            .take_while(|c| *c != '=')
            .collect::<String>()
            .trim()
            .to_lowercase();

        if key.is_empty() {
            break;
        }

        // Read value (may be quoted)
        let value = if chars.peek() == Some(&'"') {
            chars.next(); // consume opening quote
            let mut val = String::new();
            let mut escaped = false;
            for c in chars.by_ref() {
                if escaped {
                    val.push(c);
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    break;
                } else {
                    val.push(c);
                }
            }
            val
        } else {
            chars
                .by_ref()
                .take_while(|c| *c != ',' && !c.is_whitespace())
                .collect()
        };

        params.insert(key, value);
    }

    params
}

/// Generates a random client nonce.
fn generate_cnonce() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 8] = rng.random();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Computes MD5 hash and returns hex string.
fn md5_hex(input: &str) -> String {
    let digest = md5::compute(input.as_bytes());
    format!("{:x}", digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_digest_header() {
        let header =
            r#"Digest realm="test@example.com", nonce="abc123", qop="auth", algorithm=MD5"#;
        let auth = DigestAuth::parse(header).unwrap();

        assert_eq!(auth.realm, "test@example.com");
        assert_eq!(auth.nonce, "abc123");
        assert_eq!(auth.qop, Some("auth".to_string()));
        assert_eq!(auth.algorithm, "MD5");
    }

    #[test]
    fn parse_digest_header_with_opaque() {
        let header = r#"Digest realm="example", nonce="xyz", opaque="opaque123""#;
        let auth = DigestAuth::parse(header).unwrap();

        assert_eq!(auth.opaque, Some("opaque123".to_string()));
    }

    #[test]
    fn parse_digest_header_minimal() {
        let header = r#"Digest realm="test", nonce="123""#;
        let auth = DigestAuth::parse(header).unwrap();

        assert_eq!(auth.realm, "test");
        assert_eq!(auth.nonce, "123");
        assert!(auth.qop.is_none());
        assert_eq!(auth.algorithm, "MD5");
    }

    #[test]
    fn digest_authorize_generates_header() {
        let mut auth = DigestAuth {
            realm: "test".to_string(),
            nonce: "abc123".to_string(),
            opaque: None,
            qop: Some("auth".to_string()),
            algorithm: "MD5".to_string(),
            nc: 0,
        };

        let header = auth.authorize("GET", "/calendar/", "user", "pass");

        assert!(header.starts_with("Digest "));
        assert!(header.contains("username=\"user\""));
        assert!(header.contains("realm=\"test\""));
        assert!(header.contains("nonce=\"abc123\""));
        assert!(header.contains("uri=\"/calendar/\""));
        assert!(header.contains("response=\""));
        assert!(header.contains("qop=auth"));
        assert!(header.contains("nc=00000001"));
        assert!(header.contains("cnonce=\""));
    }

    #[test]
    fn basic_auth_encoding() {
        let header = basic_auth("user", "password");
        // base64("user:password") = "dXNlcjpwYXNzd29yZA=="
        assert_eq!(header, "Basic dXNlcjpwYXNzd29yZA==");
    }

    #[test]
    fn md5_hex_computation() {
        // MD5("hello") = 5d41402abc4b2a76b9719d911017c592
        assert_eq!(md5_hex("hello"), "5d41402abc4b2a76b9719d911017c592");
    }
}
