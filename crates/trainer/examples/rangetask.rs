// Prints a Range Builder task as JSON (dev tool): rangetask <table> <stack> <scenario>
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let mut an = trainer::import::Analyzer::new(std::path::Path::new("data"));
    let t = trainer::ranges::preflop_task(&mut an, a[1].parse()?, a[2].parse()?, &a[3])?;
    println!("{}", serde_json::to_string(&t)?);
    Ok(())
}
