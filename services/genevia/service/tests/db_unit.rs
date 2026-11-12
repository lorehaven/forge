//! Runs against a real Postgres with the genevia schema in place (`foreman db`). Set
//! `GENEVIA_TEST_DATABASE_URL` to run it; without it the database tests are skipped, as foundry's are.

use genevia_shared::store::{self, slug};
use genevia_shared::{
    EventEdit, FactEdit, FamilyEdit, PersonEdit, add_relative, ancestry, delete_family, delete_person, family_edit, list_trees,
    person, person_edit, save_family, save_person, search,
};
use quench_db::prelude::Db;

#[quench_test::test]
fn file_names_become_addresses() {
    assert_eq!(slug("Walus family tree.ged"), "walus_family_tree");
    assert_eq!(slug("C:\\drzewa\\Węgrzyn – rodzina.GED"), "wegrzyn_rodzina");
    assert_eq!(slug("***.ged"), "tree");
}

/// One runtime and one store for the database tests, since the store is shared by the process.
#[tokio::main(flavor = "current_thread")]
async fn database_tests() {
    let Ok(url) = std::env::var("GENEVIA_TEST_DATABASE_URL") else {
        println!("GENEVIA_TEST_DATABASE_URL is not set: skipping the database tests");
        return;
    };
    store::init(Db::connect(&url).await.expect("connects")).expect("store");
    round_trip().await;
    editing().await;
}

async fn round_trip() {

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../data/walus_family_tree.ged");
    let text = std::fs::read_to_string(path).expect("the sample file");
    let id = "genevia_test_tree";
    let _ = store::delete(id).await;

    let first = store::import("Genevia test tree.ged", &text).await.expect("imports");
    assert_eq!((first.id.as_str(), first.people, first.families, first.replaced), (id, 193, 96, false));
    // The same file again replaces the tree instead of adding a second one.
    let again = store::import("Genevia test tree.ged", &text).await.expect("imports again");
    assert!(again.replaced);

    let trees = list_trees().await.expect("trees");
    let t = trees.iter().find(|t| t.id == id).expect("listed");
    assert_eq!((t.people, t.families), (193, 96));
    assert_eq!(t.root_name, "Paweł Jan Walus");

    let a = ancestry(id.into(), String::new(), 4).await.expect("ancestry");
    assert_eq!(a.root.name, "Paweł Jan Walus");
    assert_eq!(a.slots.len(), 1 + 2 + 4 + 8 + 16 - 2);
    assert!(a.depth >= 9);
    // Dates and places come back as they went in.
    let p = person(id.into(), a.root.xref.clone()).await.expect("person");
    assert_eq!(p.father.as_ref().expect("father").name, "Krzysztof Jan Walus");
    assert_eq!(p.birth.as_ref().expect("birth").date, "3 wrz 1993");
    // The godparents, the record and the sheet the row came from are fields now, not notes.
    let birth = p.birth.as_ref().expect("birth");
    assert_eq!(birth.witnesses, vec!["Alina Łasut".to_string(), "Wojciech Węgrzyn".to_string()]);
    assert_eq!(birth.sources[0].title, "Akt urodzenia");
    assert!(birth.sources[0].detail.starts_with("USC Mikołów 1993"));
    assert_eq!(p.sources[0].title, "Arkusz kalkulacyjny");
    assert!(p.facts.iter().any(|f| f.kind == "OCCU" && f.value == "informatyk"));
    // Burial, the age at death and the cause come through for those who have them.
    let grandfather = person(id.into(), "I8".into()).await.expect("I8");
    assert_eq!(grandfather.facts.iter().find(|f| f.kind == "BURI").expect("burial").event.place, "cmentarz Mikołów");
    assert!(grandfather.facts.iter().any(|f| f.kind == "OCCU" && f.value == "kolejarz"));
    let great = person(id.into(), "I10".into()).await.expect("I10");
    assert_eq!(great.death.as_ref().expect("death").cause, "wylew");
    // Marriage and the notes under it survive too.
    let father = person(id.into(), p.father.expect("father").xref).await.expect("father");
    let marriage = father.unions[0].marriage.as_ref().expect("marriage");
    assert_eq!(marriage.date, "21 wrz 1991");
    assert!(marriage.witnesses.len() == 2);
    assert_eq!(marriage.sources[0].title, "Akt ślubu");

    let hits = search(id.into(), "laziska gal".into()).await.expect("search");
    assert!(hits.iter().any(|c| c.surname.starts_with("Gałaszek")));

    // A file with the standard tags: names, christening, burial, occupation, a cited source.
    let mh = "0 HEAD\n0 @S1@ SOUR\n1 TITL Księga chrztów\n0 @I1@ INDI\n1 NAME Anna /Kowalska/\n2 NICK Ania\n1 NAME Anna /Nowak/\n2 TYPE birth\n1 SEX F\n1 BIRT\n2 DATE 4 MAR 1901\n2 SOUR @S1@\n3 PAGE s. 12\n1 CHR\n2 DATE 10 MAR 1901\n2 PLAC Mikołów\n1 DEAT\n2 DATE 1 JAN 1980\n2 CAUS zawał\n1 BURI\n2 PLAC cmentarz\n1 OCCU nauczycielka\n2 DATE FROM 1925 TO 1960\n1 NOTE uwaga\n";
    let mid = "genevia_test_standard";
    let _ = store::delete(mid).await;
    store::import("Genevia test standard.ged", mh).await.expect("imports");
    let q = person(mid.into(), "I1".into()).await.expect("person");
    assert_eq!((q.nickname.as_str(), q.maiden_name.as_str()), ("Ania", "Nowak"));
    assert_eq!(q.death.as_ref().expect("death").cause, "zawał");
    assert_eq!(q.birth.as_ref().expect("birth").sources[0].title, "Księga chrztów");
    assert_eq!(q.birth.as_ref().expect("birth").sources[0].detail, "s. 12");
    let kinds: Vec<&str> = q.facts.iter().map(|f| f.kind.as_str()).collect();
    assert_eq!(kinds, vec!["CHR", "BURI", "OCCU"]);
    assert_eq!(q.facts[0].event.place, "Mikołów");
    assert_eq!(q.facts[2].value, "nauczycielka");
    assert_eq!(q.facts[2].event.date, "od 1925 do 1960");
    assert_eq!(q.notes, vec!["uwaga".to_string()]);
    assert!(store::delete(mid).await.expect("deletes"));

    assert!(store::import("empty.ged", "0 HEAD\n0 TRLR\n").await.is_err());
    assert!(store::delete(id).await.expect("deletes"));
    assert!(!store::delete(id).await.expect("already gone"));
}

async fn editing() {
    let id = "genevia_test_editing";
    let _ = store::delete(id).await;
    let ged = "0 HEAD\n0 @I1@ INDI\n1 NAME Jan /Kowal/\n1 SEX M\n1 FAMS @F1@\n0 @I2@ INDI\n1 NAME Anna /Nowak/\n1 SEX F\n1 FAMS @F1@\n0 @I3@ INDI\n1 NAME Piotr /Kowal/\n1 SEX M\n1 FAMC @F1@\n0 @F1@ FAM\n1 HUSB @I1@\n1 WIFE @I2@\n1 CHIL @I3@\n1 MARR\n2 DATE 1 JAN 1950\n";
    store::import("Genevia test editing.ged", ged).await.expect("imports");
    let t = id.to_string();

    // A person: change the name, the birth, add a fact and a source.
    let mut e = person_edit(t.clone(), "I1".into()).await.expect("form");
    assert_eq!(e.given, "Jan");
    e.given = "Jan Piotr".into();
    e.birth = EventEdit { date: "ok. 1920".into(), place: "Gliwice".into(), witnesses: "A B; C D".into(), sources: "Akt urodzenia: USC Gliwice 1920 nr 3".into(), ..EventEdit::default() };
    e.facts.push(FactEdit { kind: "OCCU".into(), value: "górnik".into(), date: "od 1940 do 1980".into(), ..FactEdit::default() });
    e.notes = "pierwsza\ndruga".into();
    save_person(t.clone(), "I1".into(), e).await.expect("saves");
    let p = person(t.clone(), "I1".into()).await.expect("person");
    assert_eq!(p.card.given, "Jan Piotr");
    assert_eq!(p.birth.as_ref().expect("birth").date, "ok. 1920");
    assert_eq!(p.birth.as_ref().expect("birth").witnesses, vec!["A B".to_string(), "C D".to_string()]);
    assert_eq!(p.facts[0].event.date, "od 1940 do 1980");
    assert_eq!(p.notes, vec!["pierwsza".to_string(), "druga".to_string()]);
    // The change reaches the people next to them: the wife and the son see the new name.
    assert_eq!(person(t.clone(), "I2".into()).await.expect("wife").unions[0].spouse.as_ref().expect("spouse").given, "Jan Piotr");
    assert_eq!(person(t.clone(), "I3".into()).await.expect("son").father.as_ref().expect("father").given, "Jan Piotr");
    // Bad input is refused with the field it is wrong in.
    let mut bad = person_edit(t.clone(), "I1".into()).await.expect("form");
    bad.birth.date = "32.13.1900".into();
    let err = save_person(t.clone(), "I1".into(), bad).await.expect_err("refused");
    assert!(err.message().contains("birth.date"), "{}", err.message());

    // The wedding belongs to the couple: edit it from the family, read it from either spouse.
    let form = family_edit(t.clone(), "F1".into()).await.expect("family");
    assert_eq!((form.edit.husband.as_str(), form.edit.wife.as_str()), ("I1", "I2"));
    assert_eq!(form.edit.marriage.date, "01.01.1950");
    let mut f: FamilyEdit = form.edit;
    f.marriage = EventEdit { date: "12.06.1951".into(), place: "Zabrze".into(), witnesses: "X Y; Z W".into(), ..EventEdit::default() };
    save_family(t.clone(), "F1".into(), f).await.expect("saves family");
    for who in ["I1", "I2"] {
        let m = person(t.clone(), who.into()).await.expect("person").unions[0].marriage.clone().expect("marriage");
        assert_eq!((m.date.as_str(), m.place.as_str()), ("12 cze 1951", "Zabrze"), "{who}");
        assert_eq!(m.witnesses.len(), 2);
    }

    // Relatives: a second child, a father for the husband, a mother, a spouse for the son.
    let mut daughter = PersonEdit { given: "Maria".into(), surname: "Kowal".into(), sex: "K".into(), ..PersonEdit::default() };
    daughter.birth.date = "1955".into();
    let m = add_relative(t.clone(), "I1".into(), "child".into(), String::new(), daughter, String::new()).await.expect("child");
    let kids = person(t.clone(), "I2".into()).await.expect("mother").unions[0].children.iter().map(|c| c.xref.clone()).collect::<Vec<_>>();
    assert_eq!(kids, vec!["I3".to_string(), m.clone()], "both parents see the new child");
    let father = PersonEdit { given: "Józef".into(), surname: "Kowal".into(), ..PersonEdit::default() };
    let jf = add_relative(t.clone(), "I1".into(), "father".into(), String::new(), father.clone(), String::new()).await.expect("father");
    assert_eq!(person(t.clone(), "I1".into()).await.expect("son").father.as_ref().expect("father").xref, jf);
    assert_eq!(person(t.clone(), jf.clone()).await.expect("father").card.sex, "M", "a father defaults to male");
    assert!(add_relative(t.clone(), "I1".into(), "father".into(), String::new(), father, String::new()).await.is_err(), "one father only");
    let mother = PersonEdit { given: "Ewa".into(), surname: "Zielona".into(), ..PersonEdit::default() };
    add_relative(t.clone(), "I1".into(), "mother".into(), String::new(), mother, String::new()).await.expect("mother");
    let anc = ancestry(t.clone(), "I1".into(), 3).await.expect("ancestry");
    assert_eq!(anc.slots.len(), 3, "he and both new parents");
    // Linking someone who is already in the tree.
    let wife_of_son = add_relative(t.clone(), "I3".into(), "spouse".into(), m.clone(), PersonEdit::default(), String::new()).await.expect("link");
    assert_eq!(wife_of_son, m);
    assert!(add_relative(t.clone(), "I3".into(), "spouse".into(), "I3".into(), PersonEdit::default(), String::new()).await.is_err());

    // Removing a person takes them out of the families; a union with nobody left goes.
    delete_person(t.clone(), "I3".into()).await.expect("deletes");
    assert!(person(t.clone(), "I3".into()).await.is_err());
    let after = person(t.clone(), "I1".into()).await.expect("father");
    assert!(after.unions[0].children.iter().all(|c| c.xref != "I3"));
    delete_family(t.clone(), "F1".into()).await.expect("deletes family");
    assert!(person(t.clone(), "I2".into()).await.expect("wife").unions.is_empty());
    assert!(store::delete(id).await.expect("cleans up"));
}

#[quench_test::test]
fn the_database_round_trip_and_editing() {
    database_tests();
}

quench_test::main!();
