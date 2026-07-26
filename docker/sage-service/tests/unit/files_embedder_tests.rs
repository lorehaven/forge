//! Unit tests for `files/embedder.rs`.

use sage_service::files::embedder::*;

#[test]
fn test_vector_literal() {
    assert_eq!(vector_literal(&[1.0, -0.5, 0.25]), "[1,-0.5,0.25]");
    assert_eq!(vector_literal(&[]), "[]");
}
