use super::class::Class;
use super::doc::Doc;
use super::label::Label;
use super::rng::SplitMix64;

/// First-run classes and rosters, no groupings yet.
const CLASSES: &[(&str, &[&str])] = &[
    (
        "McCurdy HR",
        &[
            "Abigail", "Aria", "Aspen", "Cooper", "Fiona", "Frank", "Haidyn", "Jack", "Jaelynn",
            "Lincoln", "Matteo", "Mia", "Nolan", "Oliver", "Stella", "Strummer", "Vaun", "Zander",
        ],
    ),
    (
        "Period 3 Science",
        &[
            "Aiden", "Bella", "Caleb", "Daisy", "Elijah", "Fatima", "Gabriel", "Hazel", "Ivan",
            "Jun", "Kai", "Lila", "Mateo", "Nora", "Owen", "Parker", "Ravi", "Sofia", "Tariq",
            "Yuki",
        ],
    ),
    (
        "Reading Club",
        &[
            "Amara", "Bodhi", "Cora", "Dante", "Eva", "Finn", "Gemma", "Hana", "Ibrahim", "Jonah",
            "Kenji", "Luna", "Malik", "Nina", "Oscar", "Pearl", "Rosa", "Silas", "Tommy", "Wren",
            "Xavier", "Yara", "Zane", "Lucy", "Omar", "Mei", "Santiago",
        ],
    ),
];

/// Fixed so every first run shows the same faces and tests can compare against `seed_doc()`.
const AVATAR_SEED: u64 = 0x5EED;

fn label(raw: &str) -> Label {
    Label::parse(raw).expect("seed names are valid labels")
}

/// Built through [`Doc::create_class`] so the seed obeys the same invariants as teacher-made
/// classes. Selects the first class.
pub fn seed_doc() -> Doc {
    let mut rng = SplitMix64::seeded(AVATAR_SEED);
    let mut doc = Doc::default();
    for (name, students) in CLASSES {
        let students = students.iter().map(|s| label(s)).collect();
        doc.create_class(label(name), students, &mut rng);
    }
    if let Some(first) = doc.classes().first().map(Class::id) {
        doc.select(first);
    }
    doc
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn seeds_three_classes_of_eighteen_to_twenty_eight_students() {
        let doc = seed_doc();
        assert_eq!(doc.classes().len(), 3);
        for class in doc.classes() {
            let n = class.students().len();
            assert!((18..=28).contains(&n), "{} has {n} students", class.name());
        }
        doc.assert_invariants();
    }

    #[test]
    fn student_names_are_distinct_within_a_class() {
        for class in seed_doc().classes() {
            let names: HashSet<_> = class.students().iter().map(|s| &s.name).collect();
            assert_eq!(names.len(), class.students().len(), "{}", class.name());
        }
    }

    #[test]
    fn the_seed_is_the_same_on_every_call() {
        assert_eq!(seed_doc(), seed_doc());
    }

    #[test]
    fn the_first_class_is_selected_and_nothing_is_grouped_yet() {
        let doc = seed_doc();
        assert_eq!(doc.selected().unwrap().id(), doc.classes()[0].id());
        assert!(doc.classes().iter().all(|c| c.grouping().is_none()));
    }
}
