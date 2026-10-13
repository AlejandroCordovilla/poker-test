// Prints the hand list, or one hand's replay, as JSON (dev tool for UI mocks):
// dumpjson <db path> [hand id]
use trainer::db::Db;
use trainer::import::*;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let db = Db::open(std::path::Path::new(&a[1]))?;
    match a.get(2) {
        Some(id) => println!("{}", serde_json::to_string(&replay(&db, id)?)?),
        None => println!("{}", serde_json::to_string(&list_hands(&db)?)?),
    }
    Ok(())
}
