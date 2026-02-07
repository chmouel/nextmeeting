//! Secret reference resolver.
//!
//! Values in `config.toml` can use special prefixes to reference secrets
//! stored outside the file:
//!
//! - `pass::path/in/store` — runs `pass show path/in/store`, returns first line
//! - `env::VAR_NAME` — reads `$VAR_NAME` from the environment
//! - anything else — returned as-is (plain text)

/// Resolves a value that may contain a secret reference prefix.
///
/// # Prefixes
///
/// - `pass::path` — executes `pass show path` and returns the first line
/// - `env::VAR` — reads environment variable `VAR`
/// - plain text — returned unchanged
pub fn resolve(value: &str) -> Result<String, String> {
    if let Some(path) = value.strip_prefix("pass::") {
        resolve_pass(path)
    } else if let Some(var) = value.strip_prefix("env::") {
        resolve_env(var)
    } else {
        Ok(value.to_string())
    }
}

/// Runs `pass show <path>` and returns the first line of stdout.
fn resolve_pass(path: &str) -> Result<String, String> {
    let output = std::process::Command::new("pass")
        .arg("show")
        .arg(path)
        .output()
        .map_err(|e| format!("failed to run `pass show {}`: {}", path, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "`pass show {}` failed (exit {}): {}",
            path,
            output.status,
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("`pass show {}` produced no output", path))
}

/// Reads an environment variable.
fn resolve_env(var: &str) -> Result<String, String> {
    std::env::var(var).map_err(|_| format!("environment variable `{}` is not set", var))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_passthrough() {
        assert_eq!(resolve("hello").unwrap(), "hello");
        assert_eq!(resolve("").unwrap(), "");
        assert_eq!(
            resolve("xxx.apps.googleusercontent.com").unwrap(),
            "xxx.apps.googleusercontent.com"
        );
    }

    #[test]
    fn env_prefix_resolves() {
        unsafe {
            std::env::set_var("_NEXTMEETING_TEST_SECRET", "my-secret-value");
        }
        assert_eq!(
            resolve("env::_NEXTMEETING_TEST_SECRET").unwrap(),
            "my-secret-value"
        );
        unsafe {
            std::env::remove_var("_NEXTMEETING_TEST_SECRET");
        }
    }

    #[test]
    fn env_prefix_missing_var_errors() {
        let result = resolve("env::_NEXTMEETING_NONEXISTENT_VAR_12345");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not set"));
    }

    #[test]
    fn pass_prefix_missing_binary_errors() {
        // This test works even if `pass` is installed — it uses an unlikely path
        // that should fail. If `pass` is not installed, it errors on the command itself.
        let result = resolve("pass::nonexistent/entry/that/should/not/exist/12345");
        assert!(result.is_err());
    }
}
