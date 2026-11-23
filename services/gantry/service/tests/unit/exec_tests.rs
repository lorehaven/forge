use gantry_service::domain::exec::{
    ExecConnection, ExecError, ExecEvent, ExecSessions, echo_shell,
};
use tokio::sync::mpsc;

/// Gives the pump task a moment to move what was sent into the buffer.
async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
}

#[quench_test::test]
async fn a_shell_prints_what_it_is_given_and_is_read_from_where_the_reader_was() {
    let sessions = ExecSessions::new();
    let id = sessions.start("ana", "ml", "web-1", echo_shell()).unwrap();
    settle().await;
    let first = sessions.read(&id, "ana", 0).unwrap();
    assert_eq!(first.data, "$ ");

    sessions.write(&id, "ana", "ls\n").await.unwrap();
    settle().await;
    let second = sessions.read(&id, "ana", first.next).unwrap();
    assert_eq!(second.data, "ls\r\n$ ");
    assert!(!second.ended);
    // Asking again from the start gets all of it.
    assert_eq!(sessions.read(&id, "ana", 0).unwrap().data, "$ ls\r\n$ ");
}

#[quench_test::test]
async fn a_shell_belongs_to_the_person_who_opened_it() {
    let sessions = ExecSessions::new();
    let id = sessions.start("ana", "ml", "web-1", echo_shell()).unwrap();
    assert!(matches!(
        sessions.read(&id, "bo", 0),
        Err(ExecError::Unknown)
    ));
    assert!(matches!(
        sessions.write(&id, "bo", "id\n").await,
        Err(ExecError::Unknown)
    ));
    sessions.close(&id, "bo");
    assert!(
        sessions.read(&id, "ana", 0).is_ok(),
        "bo could not close it"
    );
    sessions.close(&id, "ana");
    assert!(matches!(
        sessions.read(&id, "ana", 0),
        Err(ExecError::Unknown)
    ));
}

#[quench_test::test]
async fn a_shell_that_ends_says_so_and_stops_taking_input() {
    let sessions = ExecSessions::new();
    let id = sessions.start("ana", "ml", "web-1", echo_shell()).unwrap();
    sessions.write(&id, "ana", "exit\n").await.unwrap();
    settle().await;
    let read = sessions.read(&id, "ana", 0).unwrap();
    assert!(read.ended);
    assert!(matches!(
        sessions.write(&id, "ana", "ls\n").await,
        Err(ExecError::Refused(_))
    ));
}

#[quench_test::test]
async fn only_so_many_shells_are_open_at_once() {
    let sessions = ExecSessions::new();
    for _ in 0..8 {
        sessions.start("ana", "ml", "web-1", echo_shell()).unwrap();
    }
    assert!(matches!(
        sessions.start("ana", "ml", "web-1", echo_shell()),
        Err(ExecError::Refused(_))
    ));
}

#[quench_test::test]
async fn a_character_cut_in_two_waits_for_its_other_half_and_old_output_is_dropped() {
    let sessions = ExecSessions::new();
    let (input, _typed) = mpsc::channel(4);
    let (to_person, output) = mpsc::channel(16);
    let id = sessions
        .start("ana", "ml", "web-1", ExecConnection { input, output })
        .unwrap();

    // "é" is two bytes: the first arrives alone.
    let bytes = "aé".as_bytes().to_vec();
    to_person
        .send(ExecEvent::Output(bytes[..2].to_vec()))
        .await
        .unwrap();
    settle().await;
    let half = sessions.read(&id, "ana", 0).unwrap();
    assert_eq!((half.data.as_str(), half.next), ("a", 1));
    to_person
        .send(ExecEvent::Output(bytes[2..].to_vec()))
        .await
        .unwrap();
    settle().await;
    assert_eq!(sessions.read(&id, "ana", half.next).unwrap().data, "é");

    // More than the buffer keeps: a reader that fell behind resumes where it begins.
    to_person
        .send(ExecEvent::Output(vec![b'x'; 300 * 1024]))
        .await
        .unwrap();
    settle().await;
    let behind = sessions.read(&id, "ana", 0).unwrap();
    assert_eq!(behind.data.len(), 256 * 1024);
    assert!(behind.next > 256 * 1024);
}
