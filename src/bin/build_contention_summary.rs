use astralane::contention::{build_range_report, store_range_report};
use astralane::db;

const DB_PATH: &str = "astralane.db";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = db::open(DB_PATH)?;
    db::init_schema(&conn)?;

    println!("computing range report (this takes a while)...");
    let report = build_range_report(&conn)?;
    println!(
        "computed: {} blocks, {} accounts, {} programs",
        report.blocks.len(),
        report.accounts.len(),
        report.programs.len()
    );

    store_range_report(&mut conn, &report)?;
    println!("stored to contention_blocks/accounts/programs");
    Ok(())
}
