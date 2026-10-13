// Prints each opponent's logged tendencies and the profile built from them (dev tool):
// logprofiles <db path>
use trainer::db::Db;
use trainer::import::*;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let db = Db::open(std::path::Path::new(&a[1]))?;
    for s in opponent_stats(&db)? {
        println!("{:?}", s);
        println!("   -> {:?}", log_profile(&s));
    }
    Ok(())
}
