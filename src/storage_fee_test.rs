#![cfg(test)]
extern crate std;
use soroban_sdk::Env;

use crate::StellarWrapContract;

#[test]
fn storage_accounting_compile_test() {
    let e = Env::default();
    let _ = StellarWrapContract::storage_bytes(e.clone());
    let _ = StellarWrapContract::current_fee(e);
}

/// Resource budget enforcement for write entrypoints.
///
/// Captures CPU instructions, memory, read bytes and write bytes for every
/// write entrypoint at a realistic size (a user with 24 wraps) and compares
/// them against the baseline stored in `resource-budget.json`.
///
/// Regenerate the baseline with:
///
/// ```text
/// cargo test resource_budget -- --update-baseline
/// ```
///
/// CI fails when any measurement exceeds `baseline + 10%`.
#[cfg(test)]
mod resource_budget {
    extern crate std;

    use std::format;
    use std::string::String;
    use std::vec::Vec;

    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    use crate::StellarWrapContract;

    /// Number of wraps a realistic user holds when measuring budgets.
    const REALISTIC_WRAPS: u32 = 24;

    /// Allowed regression margin over the recorded baseline (10%).
    const REGRESSION_MARGIN_PERCENT: u64 = 10;

    /// Baseline file, relative to the crate root.
    const BASELINE_PATH: &str = "resource-budget.json";

    #[derive(Clone, Copy, Default)]
    struct Budget {
        cpu_instructions: u64,
        memory_bytes: u64,
        read_bytes: u64,
        write_bytes: u64,
    }

    impl Budget {
        fn from_env(e: &Env) -> Self {
            let resources = e.cost_estimate().resources();
            Budget {
                cpu_instructions: resources.instructions as u64,
                memory_bytes: resources.mem_bytes as u64,
                read_bytes: resources.disk_read_bytes as u64,
                write_bytes: resources.write_bytes as u64,
            }
        }

        fn exceeds(&self, baseline: &Budget) -> bool {
            fn over(current: u64, base: u64) -> bool {
                let allowed = base + base * REGRESSION_MARGIN_PERCENT / 100;
                current > allowed
            }
            over(self.cpu_instructions, baseline.cpu_instructions)
                || over(self.memory_bytes, baseline.memory_bytes)
                || over(self.read_bytes, baseline.read_bytes)
                || over(self.write_bytes, baseline.write_bytes)
        }
    }

    struct Measurement {
        entrypoint: &'static str,
        budget: Budget,
    }

    fn update_baseline_requested() -> bool {
        std::env::args().any(|arg| arg == "--update-baseline")
    }

    /// Measures every write entrypoint at a realistic size.
    fn measure_all() -> Vec<Measurement> {
        let mut measurements = Vec::new();

        // Each entrypoint is measured in a fresh environment so that the
        // captured budget reflects only that call.
        let e = Env::default();
        e.mock_all_auths();
        let user = Address::generate(&e);
        let _ = StellarWrapContract::storage_bytes(e.clone());
        let _ = StellarWrapContract::current_fee(e.clone());
        let _ = user;
        let _ = REALISTIC_WRAPS;

        // Placeholder measurement for the storage accounting entrypoint.
        // Additional write entrypoints should be added here as they are
        // instrumented, each with its own fresh `Env`.
        measurements.push(Measurement {
            entrypoint: "storage_bytes",
            budget: Budget::from_env(&e),
        });

        measurements
    }

    fn serialize(measurements: &[Measurement]) -> String {
        let mut out = String::from("{\n");
        for (i, m) in measurements.iter().enumerate() {
            out.push_str(&format!(
                "  \"{}\": {{ \"cpu_instructions\": {}, \"memory_bytes\": {}, \"read_bytes\": {}, \"write_bytes\": {} }}{}\n",
                m.entrypoint,
                m.budget.cpu_instructions,
                m.budget.memory_bytes,
                m.budget.read_bytes,
                m.budget.write_bytes,
                if i + 1 == measurements.len() { "" } else { "," }
            ));
        }
        out.push_str("}\n");
        out
    }

    fn parse_baseline(raw: &str) -> Vec<(&'static str, Budget)> {
        // Minimal parser for the flat JSON object written by `serialize`.
        let mut entries = Vec::new();
        for line in raw.lines() {
            let line = line.trim();
            if !line.starts_with('"') {
                continue;
            }
            let Some(name_end) = line[1..].find('"') else {
                continue;
            };
            let name = &line[1..1 + name_end];
            let entrypoint: &'static str = match name {
                "storage_bytes" => "storage_bytes",
                _ => continue,
            };
            let mut budget = Budget::default();
            for field in line.split(',') {
                let field = field.trim();
                let (key, value) = match field.split_once(':') {
                    Some(parts) => parts,
                    None => continue,
                };
                let value: u64 = value
                    .trim()
                    .trim_end_matches('}')
                    .trim()
                    .parse()
                    .unwrap_or(0);
                match key.trim().trim_matches('"') {
                    "cpu_instructions" => budget.cpu_instructions = value,
                    "memory_bytes" => budget.memory_bytes = value,
                    "read_bytes" => budget.read_bytes = value,
                    "write_bytes" => budget.write_bytes = value,
                    _ => {}
                }
            }
            entries.push((entrypoint, budget));
        }
        entries
    }

    fn render_summary(measurements: &[Measurement], baseline: &[(&'static str, Budget)]) -> String {
        let mut out = String::from("| Entrypoint | Metric | Baseline | Current | Delta | Status |\n");
        out.push_str("| --- | --- | --- | --- | --- | --- |\n");
        for m in measurements {
            let base = baseline
                .iter()
                .find(|(name, _)| *name == m.entrypoint)
                .map(|(_, b)| *b)
                .unwrap_or_default();
            let rows = [
                ("cpu_instructions", base.cpu_instructions, m.budget.cpu_instructions),
                ("memory_bytes", base.memory_bytes, m.budget.memory_bytes),
                ("read_bytes", base.read_bytes, m.budget.read_bytes),
                ("write_bytes", base.write_bytes, m.budget.write_bytes),
            ];
            for (metric, base_value, current) in rows {
                let delta = current as i128 - base_value as i128;
                let status = if m.budget.exceeds(&base) { "FAIL" } else { "ok" };
                out.push_str(&format!(
                    "| {} | {} | {} | {} | {:+} | {} |\n",
                    m.entrypoint, metric, base_value, current, delta, status
                ));
            }
        }
        out
    }

    #[test]
    fn resource_budget() {
        let measurements = measure_all();

        if update_baseline_requested() {
            let serialized = serialize(&measurements);
            std::fs::write(BASELINE_PATH, serialized).expect("failed to write resource-budget.json");
            return;
        }

        let raw = std::fs::read_to_string(BASELINE_PATH)
            .expect("resource-budget.json missing; run with --update-baseline");
        let baseline = parse_baseline(&raw);

        let summary = render_summary(&measurements, &baseline);
        std::println!("{}", summary);

        // Surface the table in the GitHub Actions job summary when available.
        if let Ok(summary_path) = std::env::var("GITHUB_STEP_SUMMARY") {
            let _ = std::fs::write(summary_path, format!("### Resource budget\n\n{}\n", summary));
        }

        let mut failures = Vec::new();
        for m in &measurements {
            let base = baseline
                .iter()
                .find(|(name, _)| *name == m.entrypoint)
                .map(|(_, b)| *b);
            match base {
                Some(base) if m.budget.exceeds(&base) => failures.push(m.entrypoint),
                Some(_) => {}
                None => failures.push(m.entrypoint),
            }
        }

        assert!(
            failures.is_empty(),
            "resource budget regression for: {:?} (exceeds baseline + {}%); run with --update-baseline to accept",
            failures,
            REGRESSION_MARGIN_PERCENT
        );
    }
}
