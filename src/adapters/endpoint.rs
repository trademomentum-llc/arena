use std::net::{Ipv4Addr, Ipv6Addr};

/// Validate that a local-inference `base_url` targets a loopback host, unless
/// remote endpoints are explicitly allowed. Prevents source code from being
/// sent to a non-loopback endpoint in local mode (NFR-4).
pub fn validate_local_endpoint(base_url: &str, allow_remote: bool) -> Result<(), String> {
    if allow_remote {
        return Ok(());
    }
    let host = extract_host(base_url)?;
    if is_loopback_host(&host) {
        Ok(())
    } else {
        Err(format!(
            "base_url host '{}' is not loopback; set allow_remote_endpoint=true to permit",
            host
        ))
    }
}

/// Extract the host from a URL authority, tolerating scheme, userinfo, port,
/// and bracketed IPv6 literals. Deterministic string parsing, no DNS.
fn extract_host(base_url: &str) -> Result<String, String> {
    let after_scheme = base_url.split("://").nth(1).unwrap_or(base_url);
    let authority = after_scheme.split('/').next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split(']').next().unwrap_or("").to_string()
    } else {
        host_port.split(':').next().unwrap_or("").to_string()
    };
    if host.is_empty() {
        Err(format!("could not parse host from base_url '{}'", base_url))
    } else {
        Ok(host)
    }
}

fn is_loopback_host(host: &str) -> bool {
    host == "localhost"
        || host.parse::<Ipv4Addr>().map(|ip| ip.is_loopback()).unwrap_or(false)
        || host.parse::<Ipv6Addr>().map(|ip| ip.is_loopback()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_localhost_ok() {
        assert!(validate_local_endpoint("http://localhost:11434/v1", false).is_ok());
    }

    #[test]
    fn loopback_ipv4_ok() {
        assert!(validate_local_endpoint("http://127.0.0.1:1234/v1", false).is_ok());
        assert!(validate_local_endpoint("http://127.5.6.7:8080", false).is_ok());
    }

    #[test]
    fn loopback_ipv6_ok() {
        assert!(validate_local_endpoint("http://[::1]:1234/v1", false).is_ok());
    }

    #[test]
    fn non_loopback_rejected() {
        assert!(validate_local_endpoint("http://10.0.0.5:11434/v1", false).is_err());
        assert!(validate_local_endpoint("https://api.example.com/v1", false).is_err());
    }

    #[test]
    fn allow_remote_bypasses() {
        assert!(validate_local_endpoint("https://api.example.com/v1", true).is_ok());
    }

    #[test]
    fn unparseable_errors() {
        assert!(validate_local_endpoint("", false).is_err());
    }
}