use crate::session::types::*;
use crate::adapters::*;
use std::fs;
use std::path::Path;
use tracing::info;

/// Spec drift detector: analyzes implementation files against spec documents
pub struct DriftDetector {
    adapter: Box<dyn AgentAdapter>,
}

impl DriftDetector {
    pub fn new(adapter: Box<dyn AgentAdapter>) -> Self {
        DriftDetector { adapter }
    }

    /// Run drift detection on a set of implementation files against specs
    pub async fn detect_drift(
        &self,
        spec_paths: &[String],
        impl_paths: &[String],
    ) -> Result<Vec<DriftFinding>, DriftError> {
        let mut findings = Vec::new();

        for spec_path in spec_paths {
            let spec_content = read_file(spec_path).map_err(|e| DriftError::Io(e.to_string()))?;
            let spec_name = Path::new(spec_path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();

            for impl_path in impl_paths {
                let impl_content =
                    read_file(impl_path).map_err(|e| DriftError::Io(e.to_string()))?;
                let impl_name = Path::new(impl_path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();

                info!(
                    spec = %spec_name,
                    implementation = %impl_name,
                    "Running drift detection"
                );

                let agent_findings = self
                    .compare_spec_vs_impl(&spec_content, &spec_name, &impl_content, &impl_name)
                    .await?;

                findings.extend(agent_findings);
            }
        }

        // Sort by severity
        findings.sort_by_key(|f| match f.severity {
            DriftSeverity::Critical => 0,
            DriftSeverity::High => 1,
            DriftSeverity::Medium => 2,
            DriftSeverity::Low => 3,
            DriftSeverity::Informational => 4,
        });

        Ok(findings)
    }

    async fn compare_spec_vs_impl(
        &self,
        spec: &str,
        spec_name: &str,
        implementation: &str,
        impl_name: &str,
    ) -> Result<Vec<DriftFinding>, DriftError> {
        let prompt = format!(
            r#"Compare the following specification against the implementation.
Identify any drift where the implementation diverges from the spec.

## Specification: {spec_name}
```
{spec}
```

## Implementation: {impl_name}
```
{implementation}
```

For each drift found, report:
1. A description of the drift
2. Severity: Critical, High, Medium, Low, or Informational

Return as JSON array:
```json
[
  {{
    "description": "...",
    "severity": "Critical|High|Medium|Low|Informational"
  }}
]
```

If no drift found, return an empty array: []"#,
            spec_name = spec_name,
            spec = spec,
            impl_name = impl_name,
            implementation = implementation
        );

        let request = AgentRequest {
            system: "You are a spec compliance auditor. Your job is to identify any drift between \
                    design specifications and their implementations. Be thorough but precise."
                .to_string(),
            prompt,
            context: None,
            output_format: OutputFormat::Json,
            temperature: 0.0,
            max_tokens: 4000,
        };

        let output = self
            .adapter
            .request(&request)
            .await
            .map_err(|e| DriftError::Agent(e.to_string()))?;

        // Parse findings from response
        let findings = parse_drift_findings(&output.content);
        Ok(findings)
    }
}

fn parse_drift_findings(content: &str) -> Vec<DriftFinding> {
    // Try to parse JSON array
    let json_result = extract_json_array(content);

    match json_result {
        Some(json) => json
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let description = item["description"].as_str()?.to_string();
                        let severity = match item["severity"].as_str().unwrap_or("Medium") {
                            "Critical" => DriftSeverity::Critical,
                            "High" => DriftSeverity::High,
                            "Medium" => DriftSeverity::Medium,
                            "Low" => DriftSeverity::Low,
                            "Informational" => DriftSeverity::Informational,
                            _ => DriftSeverity::Medium,
                        };
                        Some(DriftFinding {
                            agent_id: "drift-detector".to_string(),
                            spec_path: "N/A".to_string(),
                            implementation_path: "N/A".to_string(),
                            description,
                            severity,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        None => {
            // If no JSON found, create a single finding with the full text
            if content.trim().is_empty() || content.contains("[]") {
                vec![]
            } else {
                vec![DriftFinding {
                    agent_id: "drift-detector".to_string(),
                    spec_path: "N/A".to_string(),
                    implementation_path: "N/A".to_string(),
                    description: content.to_string(),
                    severity: DriftSeverity::Informational,
                }]
            }
        }
    }
}

fn extract_json_array(content: &str) -> Option<serde_json::Value> {
    // Try parsing whole content
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(content) {
        if json.is_array() {
            return Some(json);
        }
    }

    // Try between ```json markers
    if let Some(start) = content.find("```json") {
        let after_start = &content[start + 7..];
        if let Some(end) = after_start.find("```") {
            let json_str = after_start[..end].trim();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                if json.is_array() {
                    return Some(json);
                }
            }
        }
    }

    // Try to find [ ... ] block
    if let Some(start) = content.find('[') {
        if let Some(end) = content.rfind(']') {
            let json_str = &content[start..=end];
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                return Some(json);
            }
        }
    }

    None
}

fn read_file(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| e.to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum DriftError {
    #[error("Agent error: {0}")]
    Agent(String),
    #[error("IO error: {0}")]
    Io(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_drift_findings_json() {
        let content = r#"[
            {"description": "Missing auth middleware", "severity": "Critical"},
            {"description": "Rate limit not enforced", "severity": "High"}
        ]"#;

        let findings = parse_drift_findings(content);
        assert_eq!(findings.len(), 2);
        assert!(matches!(findings[0].severity, DriftSeverity::Critical));
        assert!(matches!(findings[1].severity, DriftSeverity::High));
    }

    #[test]
    fn test_parse_drift_findings_empty() {
        let content = "[]";
        let findings = parse_drift_findings(content);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_drift_findings_fallback() {
        let content = "No drift detected, implementation matches spec.";
        let findings = parse_drift_findings(content);
        assert_eq!(findings.len(), 1);
        assert!(matches!(findings[0].severity, DriftSeverity::Informational));
    }

    #[test]
    fn test_parse_drift_findings_with_markdown() {
        let content = r#"Here are the findings:

```json
[
    {"description": "Drift in error handling", "severity": "Medium"}
]
```"#;

        let findings = parse_drift_findings(content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].description, "Drift in error handling");
    }
}
