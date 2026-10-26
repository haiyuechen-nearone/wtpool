//! The check registry of verify/checks.toml: the layers, and so the parts, that run each check that
//! is not only an int test.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;

pub const CHECKS: &str = "verify/checks.toml";

/// The part that runs a layer's checks.
pub fn layer_part(layer: &str) -> &'static str {
    match layer {
        "static" | "render" | "host" => "quick",
        "shot" | "e2e" => "ui",
        "bench" | "bench-chromium" => "bench",
        "cc" => "cc",
        _ => "int",
    }
}

#[derive(Deserialize)]
struct ChecksFile {
    checks: BTreeMap<String, Vec<String>>,
}

/// The layers that run each check that is not only an int test.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Registry {
    pub layers: BTreeMap<String, BTreeSet<String>>,
}

impl Registry {
    /// verify/checks.toml plus the `layers` of every scenario file.
    pub fn load(root: &Path) -> Result<Self, String> {
        let path = root.join(CHECKS);
        let text = fs::read_to_string(&path).map_err(|e| format!("{CHECKS}: {e}"))?;
        let file: ChecksFile = toml::from_str(&text).map_err(|e| format!("{CHECKS}: {e}"))?;
        let mut layers: BTreeMap<String, BTreeSet<String>> = file
            .checks
            .into_iter()
            .map(|(check, layers)| (check, layers.into_iter().collect()))
            .collect();
        for name in crate::scenario::names(root) {
            let scenario = crate::scenario::load(root, &name)?;
            layers.entry(name).or_default().extend(scenario.layers);
        }
        Ok(Self { layers })
    }

    pub fn in_layer(&self, layer: &str) -> BTreeSet<String> {
        self.layers
            .iter()
            .filter(|(_, layers)| layers.contains(layer))
            .map(|(check, _)| check.clone())
            .collect()
    }

    /// Every part that runs `check`, none for a check the registry does not know.
    pub fn parts(&self, check: &str) -> BTreeSet<&'static str> {
        self.layers
            .get(check)
            .map(|layers| layers.iter().map(|layer| layer_part(layer)).collect())
            .unwrap_or_default()
    }

    /// The part a canary or freeze runs `check` in: int first, then quick, ui, bench and cc.
    pub fn part(&self, check: &str) -> Option<&'static str> {
        let parts = self.parts(check);
        ["int", "quick", "ui", "bench", "cc"]
            .into_iter()
            .find(|part| parts.contains(part))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry__should_route_scenarios_to_int_first() {
        // Given
        let registry = Registry {
            layers: BTreeMap::from([
                (
                    "idle_pool".to_owned(),
                    BTreeSet::from(["int".to_owned(), "render".to_owned()]),
                ),
                (
                    "budget__b8".to_owned(),
                    BTreeSet::from(["bench".to_owned()]),
                ),
                (
                    "csp__should_report_no_violation".to_owned(),
                    BTreeSet::from(["shot".to_owned()]),
                ),
            ]),
        };

        // When
        let parts = (
            registry.part("idle_pool"),
            registry.part("budget__b8"),
            registry.part("csp__should_report_no_violation"),
            registry.part("unknown"),
        );

        // Then
        assert_eq!(parts, (Some("int"), Some("bench"), Some("ui"), None));
    }
}
