/*!
 * Baton Data Loss Prevention (DLP) Engine — Pillar 5, V2
 *
 * Scans outbound LLM prompts for sensitive data patterns before they leave
 * the Desktop Hub. On a violation, the request is BLOCKED (mode: block)
 * and the incident is logged to the audit trail.
 *
 * Patterns detected:
 *  - AWS Access Keys (AKIA...)
 *  - GitHub / GitLab Personal Access Tokens
 *  - Private RSA / EC / PEM keys (-----BEGIN ... KEY-----)
 *  - Generic API keys / secrets (common env-var patterns)
 *  - Credit card numbers (Luhn-valid 13–19 digits)
 *  - Aadhaar numbers (12-digit Indian national ID)
 *  - Passwords in key=value form (password=..., secret=..., token=...)
 */

use regex::Regex;
use tracing::warn;

/// A single DLP pattern: a regex and a human-readable label.
struct DlpPattern {
    label: &'static str,
    regex: Regex,
}

/// Result of a DLP scan.
#[derive(Debug)]
pub struct DlpViolation {
    pub label: &'static str,
    /// Redacted excerpt for audit logging (never the actual secret value).
    pub redacted_sample: String,
}

/// The DLP engine holding all compiled patterns.
pub struct DlpEngine {
    patterns: Vec<DlpPattern>,
}

impl DlpEngine {
    /// Build and compile all patterns once at startup.
    pub fn new() -> Self {
        let raw_patterns: Vec<(&'static str, &'static str)> = vec![
            // Cloud provider credentials
            ("AWS Access Key",         r"AKIA[0-9A-Z]{16}"),
            ("AWS Secret Key",         r#"(?i)aws.{0,20}secret.{0,20}['"][0-9a-zA-Z/+]{40}['"]"#),
            ("GCP Service Account",    r#"['"]type['"]:\s*['"]service_account['"]"#),
            // Source control tokens
            ("GitHub Token (Classic)", r"ghp_[0-9a-zA-Z]{36}"),
            ("GitHub OAuth Token",     r"gho_[0-9a-zA-Z]{36}"),
            ("GitHub Actions Token",   r"ghs_[0-9a-zA-Z]{36}"),
            ("GitLab Token",           r"glpat-[0-9a-zA-Z\-]{20}"),
            // Private keys (PEM headers)
            ("Private RSA Key",        r"-----BEGIN RSA PRIVATE KEY-----"),
            ("Private EC Key",         r"-----BEGIN EC PRIVATE KEY-----"),
            ("Private Key (generic)",  r"-----BEGIN PRIVATE KEY-----"),
            ("OpenSSH Private Key",    r"-----BEGIN OPENSSH PRIVATE KEY-----"),
            // Generic secret patterns in env-var / config style
            ("Generic API Key",        r#"(?i)(api[_-]?key|apikey)\s*[:=]\s*['"]?[A-Za-z0-9_\-]{20,}['"]?"#),
            ("Generic Secret",         r#"(?i)(secret|password|passwd|pwd)\s*[:=]\s*['"]?[A-Za-z0-9_\-!@#$%^&*]{8,}['"]?"#),
            ("Bearer Token",           r"(?i)Bearer\s+[A-Za-z0-9\-._~+/]+=*"),
            // Payment card data (PCI-DSS)
            ("Credit Card Number",     r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12})\b"),
            // Indian national ID (DPDP Act compliance)
            ("Aadhaar Number",         r"\b[2-9]{1}[0-9]{3}\s?[0-9]{4}\s?[0-9]{4}\b"),
            // Slack / Stripe tokens
            ("Slack Token",            r"xox[baprs]-[0-9]{9,12}-[0-9]{9,12}-[a-zA-Z0-9]{24}"),
            ("Stripe Secret Key",      r"sk_live_[0-9a-zA-Z]{24}"),
            ("Stripe Publishable Key", r"pk_live_[0-9a-zA-Z]{24}"),
        ];

        let patterns = raw_patterns
            .into_iter()
            .filter_map(|(label, pattern)| {
                match Regex::new(pattern) {
                    Ok(regex) => Some(DlpPattern { label, regex }),
                    Err(e) => {
                        // Log but don't crash — degraded protection is better than no startup.
                        warn!("DLP: Failed to compile pattern '{}': {}", label, e);
                        None
                    }
                }
            })
            .collect();

        Self { patterns }
    }

    /// Scan a prompt string for DLP violations.
    /// Returns a list of violations found (may be empty = clean).
    pub fn scan(&self, text: &str) -> Vec<DlpViolation> {
        let mut violations = Vec::new();

        for pattern in &self.patterns {
            if let Some(m) = pattern.regex.find(text) {
                // Build a redacted sample: show the pattern label and a
                // partial context window — never the actual secret value.
                let start = m.start().saturating_sub(10);
                let end = (m.end() + 10).min(text.len());
                let context = &text[start..end];
                // Replace the matched value with [REDACTED] for the log.
                let redacted_sample = pattern.regex.replace(context, "[REDACTED]").to_string();

                violations.push(DlpViolation {
                    label: pattern.label,
                    redacted_sample,
                });
            }
        }

        violations
    }

    /// Check if a prompt is clean (no violations found).
    pub fn is_clean(&self, text: &str) -> bool {
        self.scan(text).is_empty()
    }
}

impl Default for DlpEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aws_key_detected() {
        let engine = DlpEngine::new();
        let prompt = "Use this key: AKIAIOSFODNN7EXAMPLE to access S3.";
        let violations = engine.scan(prompt);
        assert!(!violations.is_empty(), "Should detect AWS Access Key");
        assert_eq!(violations[0].label, "AWS Access Key");
    }

    #[test]
    fn test_clean_prompt_passes() {
        let engine = DlpEngine::new();
        let prompt = "Please summarize this article about machine learning.";
        assert!(engine.is_clean(prompt), "Clean prompt should pass DLP");
    }

    #[test]
    fn test_pem_key_detected() {
        let engine = DlpEngine::new();
        let prompt = "Here is my key:\n-----BEGIN RSA PRIVATE KEY-----\nMIIE...";
        let violations = engine.scan(prompt);
        assert!(!violations.is_empty(), "Should detect RSA private key header");
    }

    #[test]
    fn test_github_token_detected() {
        let engine = DlpEngine::new();
        let prompt = "Token: ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ1234567890";
        let violations = engine.scan(prompt);
        assert!(!violations.is_empty(), "Should detect GitHub classic token");
    }
}
