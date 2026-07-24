use anyhow::Result;
use surrealdb::engine::local::Mem;
use surrealdb::Surreal;
use mythrax_core::db::schema::INIT_SCHEMA;

#[tokio::test]
async fn dummy_test2() -> Result<()> {
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("test").use_db("test").await?;
    db.query(INIT_SCHEMA).await?.check()?;

    let sql1 = "SELECT (->followed_by->episode)[0..15] AS succs FROM episode;";
    db.query(sql1).await?.check()?;

    let sql2 = "SELECT ->followed_by->episode LIMIT 15 AS succs FROM episode;";
    if let Err(e) = db.query(sql2).await {
        println!("SQL2 ERROR: {:?}", e);
    }
    
    Ok(())
}
