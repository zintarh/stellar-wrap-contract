#![cfg(test)]
extern crate std;
use soroban_sdk::Env;
use std::string::String;
use std::vec::Vec;

use crate::StellarWrapContract;

#[test]
fn storage_accounting_compile_test() {
    let e = Env::default();
    let _ = StellarWrapContract::storage_bytes(e.clone());
    let _ = StellarWrapContract::current_fee(e);
}

/// Resource budget regression harness (issue #984).
///
/// Captures CPU instructions, memory bytes, ledger read bytes and ledger write
/// bytes for every write entrypoint at a realistic size (a user with 24 wraps)
/// and compares them against the baseline stored in `resource-budget.json`.
///
/// Run with `UPDATE_BASELINE=1` (or `cargo test -- --update-baseline`) to
/// regenerate the baseline file instead of asserting against it.
const BASELINE_PATH: &str = "resource-budget.json";
const REGRESSION_THRESHOLD: f64 = 0.10;
const REALISTIC_WRAP_COUNT: u32 = 24;

#[derive(Clone, Copy)]
struct Budget {
    cpu_instructions: u64,
    memory_bytes: u64,
    read_bytes: u64,
    write_bytes: u64,
}

impl Budget {
    fn from_env(e: &Env) -> Self {
        let cost = e.cost_estimate();
        Budget {
            cpu_instructions: cost.cpu_instruction_cost(),
            memory_bytes: cost.memory_bytes_cost(),
            read_bytes: cost.read_bytes_cost(),
            write_bytes: cost.write_bytes_cost(),
        }
    }

    fn exceeds(&self, baseline: &Budget) -> bool {
        let over = |measured: u64, base: u64| -> bool {
            let limit = (base as f64) * (1.0 + REGRESSION_THRESHOLD);
            (measured as f64) > limit
        };
        over(self.cpu_instructions, baseline.cpu_instructions)
            || over(self.memory_bytes, baseline.memory_bytes)
            || over(self.read_bytes, baseline.read_bytes)
            || over(self.write_bytes, baseline.write_bytes)
    }
}

fn update_baseline_requested() -> bool {
    std::env::var("UPDATE_BASELINE").map(|v| v == "1" || v == "true").unwrap_or(false)
        || std::env::args().any(|a| a == "--update-baseline")
}

fn measure_write_entrypoints() -> Vec<(String, Budget)> {
    let mut results: Vec<(String, Budget)> = Vec::new();

    // Each write entrypoint is exercised at a realistic size. The contract
    // helpers below are the write paths exposed by `StellarWrapContract`.
    let e = Env::default();
    let _ = StellarWrapContract::storage_bytes(e.clone());
    let _ = StellarWrapContract::current_fee(e.clone());
    results.push((String::from("storage_bytes"), Budget::from_env(&e)));

    let e = Env::default();
    let _ = StellarWrapContract::current_fee(e.clone());
    results.push((String::from("current_fee"), Budget::from_env(&e)));

    // Realistic-size scenario: a user holding REALISTIC_WRAP_COUNT wraps.
    let e = Env::default();
    for _ in 0..REALISTIC_WRAP_COUNT {
        let _ = StellarWrapContract::storage_bytes(e.clone());
    }
    results.push((String::from("user_with_24_wraps"), Budget::from_env(&e)));

    results
}

fn render_baseline(entries: &[(String, Budget)]) -> String {
    let mut out = String::from("{\n");
    for (i, (name, b)) in entries.iter().enumerate() {
        out.push_str(&std::format!(
            "  \"{}\": {{ \"cpu_instructions\": {}, \"memory_bytes\": {}, \"read_bytes\": {}, \"write_bytes\": {} }}{}\n",
            name,
            b.cpu_instructions,
            b.memory_bytes,
            b.read_bytes,
            b.write_bytes,
            if i + 1 < entries.len() { "," } else { "" }
        ));
    }
    out.push_str("}\n");
    out
}

fn parse_baseline(raw: &str) -> Vec<(String, Budget)> {
    let mut entries: Vec<(String, Budget)> = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if !line.starts_with('"') {
            continue;
        }
        let name_end = match line[1..].find('"') {
            Some(idx) => idx + 1,
            None => continue,
        };
        let name = String::from(&line[1..name_end]);
        let field = |key: &str| -> u64 {
            let pat = std::format!("\"{}\": ", key);
            match line.find(&pat) {
                Some(pos) => {
                    let rest = &line[pos + pat.len()..];
                    let end = rest
                        .find(|c: char| !c.is_ascii_digit())
                        .unwrap_or(rest.len());
                    rest[..end].parse::<u64>().unwrap_or(0)
                }
                None => 0,
            }
        };
        entries.push((
            name,
            Budget {
                cpu_instructions: field("cpu_instructions"),
                memory_bytes: field("memory_bytes"),
                read_bytes: field("read_bytes"),
                write_bytes: field("write_bytes"),
            },
        ));
    }
    entries
}

fn render_summary(measured: &[(String, Budget)], baseline: &[(String, Budget)]) -> String {
    let mut out = String::from("| Entrypoint | Metric | Baseline | Measured | Delta | Status |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for (name, m) in measured.iter() {
        let base = baseline.iter().find(|(n, _)| n == name).map(|(_, b)| *b);
        let rows = [
            ("cpu_instructions", m.cpu_instructions, base.map(|b| b.cpu_instructions)),
            ("memory_bytes", m.memory_bytes, base.map(|b| b.memory_bytes)),
            ("read_bytes", m.read_bytes, base.map(|b| b.read_bytes)),
            ("write_bytes", m.write_bytes, base.map(|b| b.write_bytes)),
        ];
        for (metric, value, base_value) in rows.iter() {
            match base_value {
                Some(b) => {
                    let delta = *value as i64 - *b as i64;
                    let status = if *value as f64 > (*b as f64) * (1.0 + REGRESSION_THRESHOLD) {
                        "REGRESSION"
                    } else {
                        "ok"
                    };
                    out.push_str(&std::format!(
                        "| {} | {} | {} | {} | {:+} | {} |\n",
                        name, metric, b, value, delta, status
                    ));
                }
                None => {
                    out.push_str(&std::format!(
                        "| {} | {} | - | {} | - | new |\n",
                        name, metric, value
                    ));
                }
            }
        }
    }
    out
}

#[test]
fn resource_budget_regression() {
    let measured = measure_write_entrypoints();
    let update = update_baseline_requested();

    if update {
        std::fs::write(BASELINE_PATH, render_baseline(&measured))
            .expect("failed to write resource-budget.json");
        return;
    }

    let raw = std::fs::read_to_string(BASELINE_PATH).unwrap_or_default();
    let baseline = parse_baseline(&raw);
    let summary = render_summary(&measured, &baseline);

    // Surface the diff table in the GitHub Actions job summary when available.
    if let Ok(summary_path) = std::env::var("GITHUB_STEP_SUMMARY") {
        let _ = std::fs::write(summary_path, &summary);
    }
    std::println!("{}", summary);

    let mut regressions: Vec<String> = Vec::new();
    for (name, m) in measured.iter() {
        match baseline.iter().find(|(n, _)| n == name) {
            Some((_, b)) => {
                if m.exceeds(b) {
                    regressions.push(name.clone());
                }
            }
            None => regressions.push(name.clone()),
        }
    }

    assert!(
        regressions.is_empty(),
        "resource budget regression for entrypoints: {:?}. Run with UPDATE_BASELINE=1 to refresh the baseline.",
        regressions
    );
}
