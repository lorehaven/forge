//! Unit tests for `lib.rs`.

use quench_db::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct TestModel {
    id: String,
    name: String,
}

impl Model for TestModel {
    fn table_name() -> String {
        "test_table".to_string()
    }

    fn columns() -> Vec<&'static str> {
        vec!["id", "name"]
    }
}

#[test]
fn test_model_trait() {
    assert_eq!(TestModel::table_name(), "test_table");
}
