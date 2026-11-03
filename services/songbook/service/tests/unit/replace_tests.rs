use crate::support::*;
use songbook_service::replace::swap;

#[quench_test::test]
fn the_new_file_takes_the_place_of_the_old_ones_whatever_their_extension() {
    let dir = Scratch::new();
    let data = dir.path();
    for name in ["Ann - Song.mp3", "Ann - Song.webm", "Ann - Other.mp3"] {
        std::fs::write(data.join(name), b"old").unwrap();
    }
    std::fs::create_dir(data.join(".replace")).unwrap();
    let fresh = data.join(".replace").join("Ann - Song.mp3");
    std::fs::write(&fresh, b"new").unwrap();

    let done = swap(data, &fresh, "Ann - Song").unwrap();
    assert_eq!(done.len(), 3, "two removed, one moved in: {done:?}");
    assert_eq!(std::fs::read(data.join("Ann - Song.mp3")).unwrap(), b"new");
    assert!(!data.join("Ann - Song.webm").exists());
    assert_eq!(
        std::fs::read(data.join("Ann - Other.mp3")).unwrap(),
        b"old",
        "another song is untouched"
    );
    assert!(!fresh.exists());
}

#[quench_test::test]
fn a_song_without_a_file_gets_one() {
    let dir = Scratch::new();
    let data = dir.path();
    std::fs::create_dir(data.join(".replace")).unwrap();
    let fresh = data.join(".replace").join("B - New.mp3");
    std::fs::write(&fresh, b"new").unwrap();
    assert_eq!(swap(data, &fresh, "B - New").unwrap().len(), 1);
    assert!(data.join("B - New.mp3").is_file());
}
