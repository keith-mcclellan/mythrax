use surrealdb::engine::local::Mem;
use surrealdb::Surreal;

#[tokio::main]
async fn main() {
    let db = Surreal::new::<Mem>(()).await.unwrap();
    db.use_ns("test").use_db("test").await.unwrap();
    
    let sql = "
        FOR $e IN (SELECT id FROM entity WHERE array::len(<-mentions) = 0 AND array::len(->alias_of) = 0 AND array::len(<-alias_of) = 0) {
            DELETE $e.id;
        };
    ";
    let res = db.query(sql).await.unwrap().check();
    println!("{:?}", res);
}
