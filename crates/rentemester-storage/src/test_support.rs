//! Typed-cell evidence for actual disposable native test subprocesses only.
use crate::migration::{quote, schema};
use rusqlite::Connection;
pub(crate) fn typed_tables(db: &Connection) -> serde_json::Value {
    let mut tables = serde_json::Map::new();
    for o in schema(db)
        .unwrap()
        .into_iter()
        .filter(|o| o.kind == "table")
    {
        let mut q = db
            .prepare(&format!("SELECT * FROM {}", quote(&o.name)))
            .unwrap();
        let width = q.column_count();
        let names: Vec<_> = q.column_names().into_iter().map(str::to_owned).collect();
        let mut rows = q.query([]).unwrap();
        let mut values = Vec::new();
        while let Some(r) = rows.next().unwrap() {
            let cells: Vec<_> = (0..width)
                .map(|i| match r.get_ref(i).unwrap() {
                    rusqlite::types::ValueRef::Null => serde_json::json!(["null"]),
                    rusqlite::types::ValueRef::Integer(n) => {
                        serde_json::json!(["integer", n.to_string()])
                    }
                    rusqlite::types::ValueRef::Real(n) => {
                        serde_json::json!(["real", format!("{:016x}", n.to_bits())])
                    }
                    rusqlite::types::ValueRef::Text(s) => serde_json::json!([
                        "text",
                        s.iter().map(|b| format!("{b:02x}")).collect::<String>()
                    ]),
                    rusqlite::types::ValueRef::Blob(s) => serde_json::json!([
                        "blob",
                        s.iter().map(|b| format!("{b:02x}")).collect::<String>()
                    ]),
                })
                .collect();
            values.push(cells);
        }
        tables.insert(o.name, serde_json::json!({"columns":names,"rows":values}));
    }

    serde_json::Value::Object(tables)
}
