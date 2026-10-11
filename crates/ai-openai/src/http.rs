//! The HTTP layer: one agent per provider, bearer auth, no redirects, capped bodies, and a
//! mapping from HTTP failures to [`AiError`] kinds an app can act on.
//!
//! - **Redirects are never followed.** A server cannot bounce the request, and the key with it, to
//!   another host.
//! - **The key is sent only in the `Authorization` header** to the configured base URL.
//! - **Error text is redacted** with the key before it leaves this module, and capped in length.

use std::io::Read;
use std::time::Duration;

use craft_ai_model::secret::redact;
use craft_ai_model::{AiError, ErrorKind, SecretString};
use serde_json::Value;
use ureq::Body;
use ureq::http::Response;

/// Largest error body read, in bytes.
const MAX_ERROR_BODY: usize = 64 * 1024;
/// Largest success body read, in bytes. Several large base64 images fit.
pub(crate) const MAX_BODY: usize = 128 * 1024 * 1024;

pub(crate) struct Http {
    agent: ureq::Agent,
    base: String,
    key: Option<SecretString>,
}

impl Http {
    pub fn new(
        base: String,
        key: Option<SecretString>,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_connect(Some(connect_timeout))
            .timeout_global(Some(request_timeout))
            .user_agent("craft-ai/0.1")
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
            base,
            key,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn bearer(&self) -> Option<String> {
        self.key
            .as_ref()
            .filter(|k| !k.is_empty())
            .map(|k| format!("Bearer {}", k.expose()))
    }

    pub fn get_json(&self, path: &str) -> Result<Value, AiError> {
        let mut req = self.agent.get(self.url(path));
        if let Some(b) = self.bearer() {
            req = req.header("Authorization", b);
        }
        let resp = self.checked(req.call())?;
        self.read_json(resp)
    }

    pub fn post(
        &self,
        path: &str,
        content_type: &str,
        body: &[u8],
    ) -> Result<Response<Body>, AiError> {
        let mut req = self.agent.post(self.url(path)).content_type(content_type);
        if let Some(b) = self.bearer() {
            req = req.header("Authorization", b);
        }
        self.checked(req.send(body))
    }

    pub fn read_json(&self, resp: Response<Body>) -> Result<Value, AiError> {
        let bytes =
            read_capped(resp.into_body().into_reader(), MAX_BODY).map_err(|e| self.scrub(e))?;
        serde_json::from_slice(&bytes).map_err(|_| {
            AiError::new(
                ErrorKind::Provider,
                "the server returned a reply that is not JSON",
            )
        })
    }

    /// Passes 2xx through and turns everything else into an error.
    fn checked(&self, r: Result<Response<Body>, ureq::Error>) -> Result<Response<Body>, AiError> {
        match r {
            Ok(resp) if resp.status().is_success() => Ok(resp),
            Ok(resp) => Err(self.status_error(resp)),
            Err(e) => Err(self.transport_error(&e)),
        }
    }

    fn status_error(&self, resp: Response<Body>) -> AiError {
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        let body = read_capped(resp.into_body().into_reader(), MAX_ERROR_BODY).unwrap_or_default();
        map_status(
            status,
            retry_after,
            &String::from_utf8_lossy(&body),
            self.key.as_ref(),
        )
    }

    fn transport_error(&self, e: &ureq::Error) -> AiError {
        let kind = match e {
            ureq::Error::BadUri(_) => ErrorKind::InvalidRequest,
            ureq::Error::TooManyRedirects => ErrorKind::Provider,
            _ => ErrorKind::Network,
        };
        let text = if matches!(e, ureq::Error::TooManyRedirects) {
            "the server tried to redirect the request, which is not followed".to_owned()
        } else {
            format!("could not reach the server: {e}")
        };
        self.scrub(AiError::new(kind, text))
    }

    /// Removes the key from an error before it is shown or logged.
    pub fn scrub(&self, e: AiError) -> AiError {
        scrub(e, self.key.as_ref())
    }
}

pub(crate) fn scrub(mut e: AiError, key: Option<&SecretString>) -> AiError {
    if let Some(k) = key {
        e.message = redact(&e.message, &[k]);
    }
    e
}

/// Reads at most `max` bytes; more than that is an error rather than a truncated reply.
pub(crate) fn read_capped(reader: impl Read, max: usize) -> Result<Vec<u8>, AiError> {
    let mut buf = Vec::new();
    let limit = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
    reader.take(limit).read_to_end(&mut buf).map_err(|_| {
        AiError::new(
            ErrorKind::Network,
            "the connection broke while reading the reply",
        )
    })?;
    if buf.len() > max {
        return Err(AiError::new(
            ErrorKind::Provider,
            "the server's reply was too large",
        ));
    }
    Ok(buf)
}

/// Maps an HTTP failure to an error kind and a short, redacted message.
pub(crate) fn map_status(
    status: u16,
    retry_after: Option<u64>,
    body: &str,
    key: Option<&SecretString>,
) -> AiError {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let err = parsed.as_ref().and_then(|v| v.get("error"));
    let upstream = err.map(crate::chat::error_text).unwrap_or_default();
    let code = err
        .and_then(|e| e.get("code").or_else(|| e.get("type")))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let kind = match status {
        401 | 403 => ErrorKind::Auth,
        402 => ErrorKind::Quota,
        429 if code.contains("quota") || code.contains("billing") => ErrorKind::Quota,
        429 => ErrorKind::RateLimited,
        400 | 404 | 405 | 413 | 415 | 422 => ErrorKind::InvalidRequest,
        _ => ErrorKind::Provider,
    };
    let message = if upstream.is_empty() {
        format!("the server answered {status}")
    } else {
        format!("the server answered {status}: {upstream}")
    };
    let mut e = scrub(AiError::new(kind, message), key);
    if kind == ErrorKind::RateLimited {
        e.retry_after_secs = retry_after;
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(msg: &str, code: &str) -> String {
        format!(r#"{{"error":{{"message":"{msg}","code":"{code}"}}}}"#)
    }

    #[test]
    fn statuses_map_to_actionable_kinds() {
        let k = |s, b: &str| map_status(s, None, b, None).kind;
        assert_eq!(k(401, ""), ErrorKind::Auth);
        assert_eq!(k(403, ""), ErrorKind::Auth);
        assert_eq!(k(402, ""), ErrorKind::Quota);
        assert_eq!(
            k(429, &body("slow", "rate_limit_exceeded")),
            ErrorKind::RateLimited
        );
        assert_eq!(
            k(429, &body("pay up", "insufficient_quota")),
            ErrorKind::Quota
        );
        assert_eq!(k(400, ""), ErrorKind::InvalidRequest);
        assert_eq!(k(404, ""), ErrorKind::InvalidRequest);
        assert_eq!(k(500, ""), ErrorKind::Provider);
        assert_eq!(k(503, "<html>down</html>"), ErrorKind::Provider);
    }

    #[test]
    fn retry_after_only_on_rate_limits() {
        assert_eq!(map_status(429, Some(7), "", None).retry_after_secs, Some(7));
        assert_eq!(map_status(500, Some(7), "", None).retry_after_secs, None);
    }

    #[test]
    fn echoed_keys_are_redacted_and_long_messages_capped() {
        let key = SecretString::new("sk-live-needle");
        let e = map_status(
            401,
            None,
            &body("Incorrect API key: sk-live-needle", "x"),
            Some(&key),
        );
        assert!(!e.message.contains("needle"), "{}", e.message);
        assert!(e.message.contains("[redacted]"));
        let long = "x".repeat(5000);
        assert!(map_status(500, None, &body(&long, "x"), None).message.len() < 400);
    }

    #[test]
    fn non_json_error_bodies_do_not_leak_or_panic() {
        let e = map_status(502, None, "<html>bad gateway</html>", None);
        assert_eq!(e.message, "the server answered 502");
    }

    #[test]
    fn read_capped_refuses_more_than_the_cap() {
        assert!(read_capped(&b"12345"[..], 5).is_ok());
        assert!(read_capped(&b"123456"[..], 5).is_err());
    }
}
