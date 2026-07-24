use anyhow::Result;
use surrealdb::engine::local::Mem;
use surrealdb::Surreal;
use mythrax_core::db::schema::INIT_SCHEMA;

#[tokio::test]
async fn dummy_test() -> Result<()> {
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("test").use_db("test").await?;
    db.query(INIT_SCHEMA).await?.check()?;

    let sql1 = "UPSERT idf_index:apple_general SET term = 'apple', scope = 'general', document_frequency = (document_frequency ?? 0) + 1;";
    db.query(sql1).await?.check()?;

    let sql2 = "UPSERT idf_index:apple_general SET term = 'apple', scope = 'general', document_frequency = (document_frequency ?? 0) + 1;";
    db.query(sql2).await?.check()?;

    let sql3 = "SELECT VALUE document_frequency FROM idf_index WHERE term = 'apple' AND scope = 'general';";
    let mut response = db.query(sql3).await?;
    let res: Option<i64> = response.take(0)?;
    println!("DF: {:?}", res);
    assert_eq!(res, Some(2));
    Ok(())
}
