use crate::support::MemorySource;
use lectern_service::library::Library;

#[quench_test::test]
async fn a_release_in_the_storage_is_loaded_and_a_newer_one_replaces_it() {
    let source = MemorySource::default();
    source.release("forge", "1", &[("index.html", "one")], None);
    let library = Library::new("/");

    let report = library.refresh(&source).await.unwrap();
    assert_eq!(report.loaded, vec!["forge 1".to_string()]);
    assert_eq!(library.get("forge").unwrap().version, "1");

    // nothing new, nothing done
    assert!(library.refresh(&source).await.unwrap().loaded.is_empty());

    source.release("forge", "2", &[("index.html", "two")], None);
    let report = library.refresh(&source).await.unwrap();
    assert_eq!(report.loaded, vec!["forge 2".to_string()]);
    assert_eq!(library.get("forge").unwrap().version, "2");
}

#[quench_test::test]
async fn a_book_that_leaves_the_storage_leaves_the_library() {
    let source = MemorySource::default();
    source.release("forge", "1", &[("index.html", "x")], None);
    source.release("quench", "1", &[("index.html", "y")], None);
    let library = Library::new("/");
    library.refresh(&source).await.unwrap();
    assert_eq!(library.list().len(), 2);

    source.remove_book("quench");
    let report = library.refresh(&source).await.unwrap();
    assert_eq!(report.removed, vec!["quench".to_string()]);
    assert!(library.get("quench").is_none() && library.get("forge").is_some());
}

#[quench_test::test]
async fn a_release_that_cannot_be_read_leaves_the_book_as_it_was() {
    let source = MemorySource::default();
    source.release("forge", "1", &[("index.html", "good")], None);
    let library = Library::new("/");
    library.refresh(&source).await.unwrap();

    // version 2 is announced, but its archive is not there
    source.release("forge", "2", &[("index.html", "new")], None);
    source.put("forge/2/forge-2.tar.gz", b"not an archive");
    let report = library.refresh(&source).await.unwrap();
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert!(report.failed[0].starts_with("forge:"), "{report:?}");
    assert_eq!(library.get("forge").unwrap().version, "1");
}

#[quench_test::test]
async fn an_unreachable_storage_changes_nothing_and_is_not_ready() {
    let source = MemorySource::default();
    source.release("forge", "1", &[("index.html", "x")], None);
    let library = Library::new("/");
    assert!(!library.is_ready());

    *source.down.lock().unwrap() = true;
    assert!(library.refresh(&source).await.is_err());
    assert!(!library.is_ready());

    *source.down.lock().unwrap() = false;
    library.refresh(&source).await.unwrap();
    assert!(library.is_ready());

    *source.down.lock().unwrap() = true;
    assert!(library.refresh(&source).await.is_err());
    assert!(
        library.get("forge").is_some(),
        "a failed look keeps what is held"
    );
}

#[quench_test::test]
async fn links_are_rewritten_for_where_the_service_is_mounted() {
    let source = MemorySource::default();
    source.release(
        "forge",
        "1",
        &[("index.html", r#"<a href="/x">x</a>"#)],
        None,
    );
    let library = Library::new("/lectern");
    library.refresh(&source).await.unwrap();
    let book = library.get("forge").unwrap();
    let lectern_service::book::Found::File(index) = book.find("") else {
        panic!("no index")
    };
    assert_eq!(index.body.as_ref(), br#"<a href="/lectern/forge/x">x</a>"#);
}

#[quench_test::test]
async fn a_book_cannot_take_a_path_of_the_service() {
    let source = MemorySource::default();
    source.release("ui", "1", &[("index.html", "x")], None);
    source.release("forge", "1", &[("index.html", "x")], None);
    let library = Library::new("/");
    let report = library.refresh(&source).await.unwrap();
    assert_eq!(report.loaded, vec!["forge 1".to_string()]);
    assert!(
        report
            .failed
            .iter()
            .any(|f| f.starts_with("ui:") && f.contains("path of the service")),
        "{report:?}"
    );
    assert!(library.get("ui").is_none());
}
