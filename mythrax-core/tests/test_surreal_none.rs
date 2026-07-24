use anyhow::Result;
use surrealdb::engine::local::Mem;
use surrealdb::Surreal;

#[tokio::test]
async fn dummy_test3() -> Result<()> {
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("test").use_db("test").await?;
    
    // Create an episode without content_hash
    db.query("INSERT INTO episode { id: 'episode:123', title: 'abc' };").await?.check()?;

    let mut res = db.query("SELECT id FROM episode WHERE content_hash = NONE;").await?;
    let r1: Vec<serde_json::Value> = res.take(0)?;
    println!("With = NONE: {:?}", r1);

    let mut res = db.query("SELECT id FROM episode WHERE content_hash IS NONE;").await?;
    let r2: Vec<serde_json::Value> = res.take(0)?;
    println!("With IS NONE: {:?}", r2);

    Ok(())
}
