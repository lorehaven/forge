//! The API against a real Postgres: import, read, edit, and who may do which. One runtime and one store for all of it, since
//! the store is shared by the process.

use crate::support::{self, Api, read};
use genevia_core::wire::{
    Ancestry, Card, EventEdit, FactEdit, FamilyEdit, FamilyForm, Imported, Me, PersonDetail,
    PersonEdit, Problem, RelativeBody, TreeInfo,
};
use quench_http::http::{Method, StatusCode};

/// Three generations: Jan (I1) with parents (I4, I5), a wife (I2) and a son (I3).
const FAMILY: &str = "0 HEAD\n0 @I1@ INDI\n1 NAME Jan /Kowal/\n1 SEX M\n1 BIRT\n2 DATE 3 SEP 1920\n2 PLAC Gliwice\n2 _WITN Ewa Zielona\n2 SOUR Akt urodzenia\n3 PAGE USC Gliwice 1920 nr 3\n1 OCCU górnik\n1 FAMC @F2@\n1 FAMS @F1@\n0 @I2@ INDI\n1 NAME Anna /Nowak/\n1 SEX F\n1 FAMS @F1@\n0 @I3@ INDI\n1 NAME Piotr /Kowal/\n1 SEX M\n1 FAMC @F1@\n0 @I4@ INDI\n1 NAME Józef /Kowal/\n1 SEX M\n1 FAMS @F2@\n0 @I5@ INDI\n1 NAME Maria /Lis/\n1 SEX F\n1 FAMS @F2@\n0 @F1@ FAM\n1 HUSB @I1@\n1 WIFE @I2@\n1 CHIL @I3@\n1 MARR\n2 DATE 1 JAN 1950\n0 @F2@ FAM\n1 HUSB @I4@\n1 WIFE @I5@\n1 CHIL @I1@\n0 TRLR\n";

const STANDARD: &str = "0 HEAD\n0 @S1@ SOUR\n1 TITL Księga chrztów\n0 @I1@ INDI\n1 NAME Anna /Kowalska/\n2 NICK Ania\n1 NAME Anna /Nowak/\n2 TYPE birth\n1 SEX F\n1 BIRT\n2 DATE 4 MAR 1901\n2 SOUR @S1@\n3 PAGE s. 12\n1 CHR\n2 DATE 10 MAR 1901\n2 PLAC Mikołów\n1 DEAT\n2 DATE 1 JAN 1980\n2 CAUS zawał\n1 BURI\n2 PLAC cmentarz\n1 OCCU nauczycielka\n2 DATE FROM 1925 TO 1960\n1 NOTE uwaga\n";

fn tree_path(tree: &str, rest: &str) -> String {
    format!("/api/v1/trees/{tree}{rest}")
}

async fn clean(tree: &str) {
    let _ = genevia_service::store::delete(tree).await;
}

async fn import_and_read(api: &Api) {
    let id = "genevia_test_family";
    clean(id).await;
    let (status, body) = api.import("Genevia test family.ged", FAMILY, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let done: Imported = read(&body);
    assert_eq!(
        (done.id.as_str(), done.people, done.families, done.replaced),
        (id, 5, 2, false)
    );
    let (_, body) = api.import("Genevia test family.ged", FAMILY, None).await;
    assert!(
        read::<Imported>(&body).replaced,
        "the same file again replaces the tree"
    );

    let (_, body) = api.get("/api/v1/trees").await;
    let trees: Vec<TreeInfo> = read(&body);
    let t = trees.iter().find(|t| t.id == id).expect("listed");
    assert_eq!((t.people, t.families), (5, 2));

    let (status, body) = api
        .get(&tree_path(id, "/ancestry?root=I1&generations=4"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let a: Ancestry = read(&body);
    assert_eq!(a.root.name, "Jan Kowal");
    assert_eq!(a.slots.len(), 3, "he and both parents");

    let (_, body) = api.get(&tree_path(id, "/people/I1")).await;
    let p: PersonDetail = read(&body);
    assert_eq!(p.father.as_ref().expect("father").name, "Józef Kowal");
    let birth = p.birth.as_ref().expect("birth");
    assert_eq!(
        (birth.date.as_str(), birth.place.as_str()),
        ("3 wrz 1920", "Gliwice")
    );
    assert_eq!(birth.witnesses, vec!["Ewa Zielona".to_string()]);
    assert_eq!(birth.sources[0].title, "Akt urodzenia");
    assert!(
        p.facts
            .iter()
            .any(|f| f.kind == "OCCU" && f.value == "górnik")
    );

    let (_, body) = api.get(&tree_path(id, "/search?q=gliw")).await;
    let hits: Vec<Card> = read(&body);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].xref, "I1");

    // What is not there is not found, and bad uploads say why.
    assert_eq!(
        api.get(&tree_path(id, "/people/I99")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.get(&tree_path("genevia_test_nothing", "/people/I1"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (status, body) = api.import("empty.ged", "0 HEAD\n0 TRLR\n", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_ne!(read::<Problem>(&body).error, "");

    // A file with the standard tags.
    let sid = "genevia_test_standard";
    clean(sid).await;
    assert_eq!(
        api.import("Genevia test standard.ged", STANDARD, None)
            .await
            .0,
        StatusCode::OK
    );
    let (_, body) = api.get(&tree_path(sid, "/people/I1")).await;
    let q: PersonDetail = read(&body);
    assert_eq!(
        (q.nickname.as_str(), q.maiden_name.as_str()),
        ("Ania", "Nowak")
    );
    assert_eq!(q.death.as_ref().expect("death").cause, "zawał");
    assert_eq!(
        q.birth.as_ref().expect("birth").sources[0].title,
        "Księga chrztów"
    );
    let kinds: Vec<&str> = q.facts.iter().map(|f| f.kind.as_str()).collect();
    assert_eq!(kinds, vec!["CHR", "BURI", "OCCU"]);
    assert_eq!(q.facts[2].event.date, "od 1925 do 1960");
    clean(sid).await;
    clean(id).await;
}

async fn editing(api: &Api) {
    let id = "genevia_test_editing";
    clean(id).await;
    assert_eq!(
        api.import("Genevia test editing.ged", FAMILY, None).await.0,
        StatusCode::OK
    );
    let person = |x: &str| tree_path(id, &format!("/people/{x}"));

    // A person: change the name, the birth, add a fact and a source.
    let (status, body) = api.get(&format!("{}/edit", person("I1"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut e: PersonEdit = read(&body);
    assert_eq!(e.given, "Jan");
    e.given = "Jan Piotr".into();
    e.birth = EventEdit {
        date: "ok. 1920".into(),
        place: "Gliwice".into(),
        witnesses: "A B; C D".into(),
        sources: "Akt urodzenia: USC Gliwice 1920 nr 3".into(),
        ..EventEdit::default()
    };
    e.facts.push(FactEdit {
        kind: "OCCU".into(),
        value: "kolejarz".into(),
        date: "od 1940 do 1980".into(),
        ..FactEdit::default()
    });
    e.notes = "pierwsza\ndruga".into();
    let (status, body) = api.send_json(Method::PUT, &person("I1"), None, &e).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let p: PersonDetail = read(&api.get(&person("I1")).await.1);
    assert_eq!(p.card.given, "Jan Piotr");
    assert_eq!(
        p.birth.as_ref().expect("birth").witnesses,
        vec!["A B".to_string(), "C D".to_string()]
    );
    assert_eq!(p.notes, vec!["pierwsza".to_string(), "druga".to_string()]);
    // Everyone who points at them sees it: the wife and the son.
    let wife: PersonDetail = read(&api.get(&person("I2")).await.1);
    assert_eq!(
        wife.unions[0].spouse.as_ref().expect("spouse").given,
        "Jan Piotr"
    );
    let son: PersonDetail = read(&api.get(&person("I3")).await.1);
    assert_eq!(son.father.as_ref().expect("father").given, "Jan Piotr");

    // Bad input is refused with the field it is wrong in.
    let mut bad: PersonEdit = read(&api.get(&format!("{}/edit", person("I1"))).await.1);
    bad.birth.date = "32.13.1900".into();
    let (status, body) = api.send_json(Method::PUT, &person("I1"), None, &bad).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        read::<Problem>(&body).field.contains("birth.date"),
        "{body}"
    );

    // The wedding belongs to the couple: edit it from the family, read it from either spouse.
    let family = |f: &str| tree_path(id, &format!("/families/{f}"));
    let form: FamilyForm = read(&api.get(&format!("{}/edit", family("F1"))).await.1);
    assert_eq!(
        (form.edit.husband.as_str(), form.edit.wife.as_str()),
        ("I1", "I2")
    );
    let mut f: FamilyEdit = form.edit;
    f.marriage = EventEdit {
        date: "12.06.1951".into(),
        place: "Zabrze".into(),
        witnesses: "X Y; Z W".into(),
        ..EventEdit::default()
    };
    assert_eq!(
        api.send_json(Method::PUT, &family("F1"), None, &f).await.0,
        StatusCode::NO_CONTENT
    );
    for who in ["I1", "I2"] {
        let d: PersonDetail = read(&api.get(&person(who)).await.1);
        let m = d.unions[0].marriage.clone().expect("marriage");
        assert_eq!(
            (m.date.as_str(), m.place.as_str()),
            ("12 cze 1951", "Zabrze"),
            "{who}"
        );
    }

    // Relatives: a second child, a mother and a father who are already there, and a spouse for the son.
    let add = |of: &str, relation: &str, existing: &str, person: PersonEdit| {
        let body = RelativeBody {
            relation: relation.into(),
            existing: existing.into(),
            person,
            family: String::new(),
        };
        let path = tree_path(id, &format!("/people/{of}/relatives"));
        async move { api.send_json(Method::POST, &path, None, &body).await }
    };
    let daughter = PersonEdit {
        given: "Maria".into(),
        surname: "Kowal".into(),
        sex: "K".into(),
        ..PersonEdit::default()
    };
    let (status, body) = add("I1", "child", "", daughter).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let new: String = read(&body);
    let mother: PersonDetail = read(&api.get(&person("I2")).await.1);
    let kids: Vec<_> = mother.unions[0]
        .children
        .iter()
        .map(|c| c.xref.clone())
        .collect();
    assert_eq!(
        kids,
        vec!["I3".to_string(), new.clone()],
        "both parents see the new child"
    );
    let (status, _) = add("I3", "spouse", &new, PersonEdit::default()).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "link someone who is already in the tree"
    );
    let (status, _) = add("I3", "spouse", "I3", PersonEdit::default()).await;
    assert_ne!(status, StatusCode::CREATED, "nobody is their own spouse");
    let one = PersonEdit {
        given: "Adam".into(),
        surname: "Kowal".into(),
        ..PersonEdit::default()
    };
    assert_ne!(
        add("I1", "father", "", one).await.0,
        StatusCode::CREATED,
        "one father only"
    );

    // Removing a person takes them out of the families; a union with nobody left goes.
    assert_eq!(
        api.call(Method::DELETE, &person("I3"), None, None).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(api.get(&person("I3")).await.0, StatusCode::NOT_FOUND);
    let after: PersonDetail = read(&api.get(&person("I1")).await.1);
    assert!(after.unions[0].children.iter().all(|c| c.xref != "I3"));
    assert_eq!(
        api.call(Method::DELETE, &family("F1"), None, None).await.0,
        StatusCode::NO_CONTENT
    );
    let wife: PersonDetail = read(&api.get(&person("I2")).await.1);
    assert_eq!(wife.unions, [] as [genevia_core::wire::UnionView; 0]);
    clean(id).await;
}

async fn permissions(api: &Api, open: &Api) {
    let mine = "genevia_test_mine";
    let theirs = "genevia_test_theirs";
    for id in [mine, theirs] {
        clean(id).await;
        let name = format!("{}.ged", id.replace('_', " "));
        assert_eq!(
            open.import(&name, FAMILY, None).await.0,
            StatusCode::OK,
            "{id}"
        );
    }
    let get = |path: String, scope: Option<&'static str>| async move {
        api.call(Method::GET, &path, scope, None).await
    };
    let list = |scope: Option<&'static str>| async move {
        let (status, body) = api.call(Method::GET, "/api/v1/trees", scope, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        read::<Vec<TreeInfo>>(&body)
            .into_iter()
            .map(|t| t.id)
            .filter(|i| i.starts_with("genevia_test_"))
            .collect::<Vec<_>>()
    };

    // Nobody signed in: nothing (the real `Auth` answers 401 before this; the API itself also refuses).
    assert_eq!(
        get(tree_path(mine, "/people/I1"), None).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(list(None).await, [] as [std::string::String; 0]);
    // A blanket read opens every tree, changes none, and does not offer the import.
    let me: Me = read(&get("/api/v1/me".into(), Some("genevia:read")).await.1);
    assert_eq!((me.name.as_str(), me.write), ("alice", false));
    assert_eq!(list(Some("genevia:read")).await.len(), 2);
    let detail: PersonDetail = read(
        &get(tree_path(mine, "/people/I1"), Some("genevia:read"))
            .await
            .1,
    );
    assert!(!detail.editable);
    assert_eq!(
        api.send_json(
            Method::PUT,
            &tree_path(mine, "/people/I1"),
            Some("genevia:read"),
            &PersonEdit::default()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        api.call(
            Method::DELETE,
            &tree_path(mine, "/people/I1"),
            Some("genevia:read"),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        api.import("Genevia test mine.ged", FAMILY, Some("genevia:read"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    // A grant on one tree: that tree only, and its edit controls.
    let branch = Some("genevia:tree:genevia_test_mine:write");
    assert_eq!(list(branch).await, vec![mine.to_string()]);
    assert_eq!(
        get(tree_path(mine, "/ancestry"), branch).await.0,
        StatusCode::OK
    );
    assert_eq!(
        get(tree_path(theirs, "/ancestry"), branch).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(tree_path(theirs, "/search?q=jan"), branch).await.0,
        StatusCode::FORBIDDEN
    );
    let detail: PersonDetail = read(&get(tree_path(mine, "/people/I1"), branch).await.1);
    assert!(detail.editable);
    let me: Me = read(&get("/api/v1/me".into(), branch).await.1);
    assert!(!me.write, "a branch does not let you add trees");
    // It may replace its own tree by importing the same file, not add another.
    assert_eq!(
        api.import("Genevia test mine.ged", FAMILY, branch).await.0,
        StatusCode::OK
    );
    assert_eq!(
        api.import("Genevia test theirs.ged", FAMILY, branch)
            .await
            .0,
        StatusCode::FORBIDDEN,
        "not a tree it has a grant on"
    );
    assert_eq!(
        api.import("Genevia test new.ged", FAMILY, branch).await.0,
        StatusCode::FORBIDDEN,
        "a branch cannot add trees"
    );
    for id in [mine, theirs] {
        clean(id).await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn database_tests() {
    let Some(url) = support::url() else {
        println!("GENEVIA_TEST_DATABASE_URL is not set: skipping the database tests");
        return;
    };
    support::connect(&url).await;
    let (open, enforced) = (Api::new(false).await, Api::new(true).await);
    import_and_read(&open).await;
    editing(&open).await;
    permissions(&enforced, &open).await;
}

#[quench_test::test]
fn the_api_against_a_database() {
    database_tests();
}
