use astralane::contention::build_range_report;
use astralane::db;

const DB_PATH: &str = "astralane.db";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = db::open_read_only(DB_PATH)?;
    let report = build_range_report(&conn)?;

    let depths: Vec<usize> = report.blocks.iter().map(|b| b.schedule_depth).collect();
    let min_depth = depths.iter().min().copied().unwrap_or(0);
    let max_depth = depths.iter().max().copied().unwrap_or(0);
    let avg_depth = if depths.is_empty() {
        0.0
    } else {
        depths.iter().sum::<usize>() as f64 / depths.len() as f64
    };

    let widths: Vec<usize> = report
        .blocks
        .iter()
        .flat_map(|b| b.step_widths.iter().copied())
        .collect();
    let max_width = widths.iter().max().copied().unwrap_or(0);
    let avg_width = if widths.is_empty() {
        0.0
    } else {
        widths.iter().sum::<usize>() as f64 / widths.len() as f64
    };

    println!("=== Contention report over {} blocks ===\n", report.blocks.len());
    println!("Schedule depth (steps per block): min={min_depth} max={max_depth} avg={avg_depth:.1}");
    println!("Step width (tx per step):         max={max_width} avg={avg_width:.1}\n");

    println!("Top 15 accounts by conflict count:");
    println!("{:<46} {:>10} {:>10}", "account", "write", "read");
    for a in report.accounts.iter().take(15) {
        println!(
            "{:<46} {:>10} {:>10}",
            a.account_pubkey, a.write_conflicts, a.read_conflicts
        );
    }

    println!("\nTop 15 programs by conflict count:");
    println!("{:<46} {:>10} {:>10}", "program", "write", "read");
    for p in report.programs.iter().take(15) {
        println!(
            "{:<46} {:>10} {:>10}",
            p.program_id, p.write_conflicts, p.read_conflicts
        );
    }

    Ok(())
}
