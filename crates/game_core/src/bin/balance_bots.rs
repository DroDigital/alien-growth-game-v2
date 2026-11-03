//! `cargo bots [--runs N] [--seed S] [--max-secs T] [--json out.json] [--assert-max-cause 0.4]`
//!
//! Plays N headless runs with the balance bots and prints the death-cause
//! distribution. With `--assert-max-cause`, exits non-zero if any single
//! death cause exceeds that share of deaths (the acceptance criterion).

use game_core::balance::Balance;
use game_core::bots::{BatchReport, run_batch};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    let mut runs = 1000usize;
    let mut seed = 1u64;
    let mut max_secs = 2400.0f32;
    let mut json: Option<PathBuf> = None;
    let mut assert_max: Option<f32> = None;
    let mut data_dir = game_core::paths::data_dir().join("balance");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_default();
        match a.as_str() {
            "--runs" => runs = val().parse().unwrap_or(runs),
            "--seed" => seed = val().parse().unwrap_or(seed),
            "--max-secs" => max_secs = val().parse().unwrap_or(max_secs),
            "--json" => json = Some(PathBuf::from(val())),
            "--assert-max-cause" => assert_max = val().parse().ok(),
            "--data" => data_dir = PathBuf::from(val()),
            other => {
                eprintln!("unknown argument {other}");
                return ExitCode::from(2);
            }
        }
    }
    let balance = match Balance::load(&data_dir) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot load balance from {}: {e}", data_dir.display());
            return ExitCode::FAILURE;
        }
    };
    let start = Instant::now();
    let results = run_batch(&balance, runs, seed, max_secs);
    let report = BatchReport::from_results(&results);
    report.print();
    println!("({:.1}s wall time)", start.elapsed().as_secs_f32());
    if let Some(path) = json {
        let body = serde_json::json!({ "report": report, "runs": results });
        if let Err(e) = std::fs::write(
            &path,
            serde_json::to_string_pretty(&body).unwrap_or_default(),
        ) {
            eprintln!("cannot write {}: {e}", path.display());
        }
    }
    if let Some(max) = assert_max
        && report.max_cause_share > max
    {
        eprintln!(
            "FAIL: death cause '{}' is {:.1}% of deaths (limit {:.0}%)",
            report.max_cause,
            report.max_cause_share * 100.0,
            max * 100.0
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
