use serde::{Deserialize, Serialize};

use super::class::{Class, ClassId};
use super::label::Label;

/// Everything that is persisted, and nothing else. Mode, timer, and the oracle announcement are
/// UI state. `selected` names a real class whenever any class exists, and is `None` otherwise.
/// Ids are never reused, so `next_class` only grows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Doc {
    classes: Vec<Class>,
    selected: Option<ClassId>,
    next_class: u32,
}

impl Doc {
    pub fn classes(&self) -> &[Class] {
        &self.classes
    }

    pub fn selected(&self) -> Option<&Class> {
        self.classes.get(self.position(self.selected?)?)
    }

    pub fn selected_mut(&mut self) -> Option<&mut Class> {
        let position = self.position(self.selected?)?;
        self.classes.get_mut(position)
    }

    /// Does nothing for an unknown id.
    pub fn select(&mut self, id: ClassId) {
        if self.position(id).is_some() {
            self.selected = Some(id);
        }
    }

    /// Appends the class and selects it.
    pub fn create_class(&mut self, name: Label, students: Vec<Label>) -> ClassId {
        let id = ClassId(self.next_class);
        self.next_class += 1;
        self.classes.push(Class::new(id, name, students));
        self.selected = Some(id);
        id
    }

    /// Idempotent. Deleting the selected class selects the class after it, or the one before it
    /// when it was last, or nothing when it was the only class.
    pub fn delete_class(&mut self, id: ClassId) {
        let Some(position) = self.position(id) else {
            return;
        };
        self.classes.remove(position);
        if self.selected == Some(id) {
            self.selected = self
                .classes
                .get(position)
                .or_else(|| self.classes.last())
                .map(Class::id);
        }
    }

    /// Restores every cross-reference after deserializing: each class reconciles its grouping and
    /// oracle, counters move above every existing id, and `selected` names a real class (the
    /// first one when it did not). Idempotent.
    pub fn reconcile(&mut self) {
        for class in &mut self.classes {
            class.reconcile();
        }
        self.next_class = self
            .classes
            .iter()
            .map(|c| c.id().0.saturating_add(1))
            .fold(self.next_class, u32::max);
        let mut seen = std::collections::BTreeSet::new();
        for class in &mut self.classes {
            if !seen.insert(class.id()) {
                class.set_id(ClassId(self.next_class));
                self.next_class += 1;
            }
        }
        if self.selected().is_none() {
            self.selected = self.classes.first().map(Class::id);
        }
    }

    fn position(&self, id: ClassId) -> Option<usize> {
        self.classes.iter().position(|c| c.id() == id)
    }

    #[cfg(test)]
    pub(crate) fn assert_invariants(&self) {
        let mut seen = std::collections::BTreeSet::new();
        for class in &self.classes {
            assert!(seen.insert(class.id()), "duplicate class id");
            assert!(class.id().0 < self.next_class, "counter not above every id");
            class.assert_invariants();
        }
        match self.selected {
            Some(id) => assert!(seen.contains(&id), "selected class does not exist"),
            None => assert!(
                self.classes.is_empty(),
                "classes exist but none is selected"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::testing::{label, labels};

    fn doc_with(names: &[&str]) -> (Doc, Vec<ClassId>) {
        let mut doc = Doc::default();
        let ids = names
            .iter()
            .map(|name| doc.create_class(label(name), labels(&["Ava", "Ben"])))
            .collect();
        (doc, ids)
    }

    fn selected_name(doc: &Doc) -> Option<&str> {
        doc.selected().map(|c| c.name().as_str())
    }

    #[test]
    fn a_new_doc_has_no_classes_and_nothing_selected() {
        let doc = Doc::default();
        assert!(doc.classes().is_empty());
        assert!(doc.selected().is_none());
        doc.assert_invariants();
    }

    #[test]
    fn creating_a_class_appends_and_selects_it() {
        let (doc, ids) = doc_with(&["A", "B"]);
        let names: Vec<&str> = doc.classes().iter().map(|c| c.name().as_str()).collect();
        assert_eq!(names, ["A", "B"]);
        assert_eq!(doc.selected().unwrap().id(), ids[1]);
        doc.assert_invariants();
    }

    #[test]
    fn select_switches_class_and_ignores_unknown_ids() {
        let (mut doc, ids) = doc_with(&["A", "B", "C"]);
        doc.select(ids[0]);
        assert_eq!(selected_name(&doc), Some("A"));
        doc.select(ClassId(99));
        assert_eq!(selected_name(&doc), Some("A"));
    }

    #[test]
    fn selected_mut_edits_the_selected_class() {
        let (mut doc, ids) = doc_with(&["A", "B"]);
        doc.select(ids[0]);
        doc.selected_mut().unwrap().rename(label("Renamed"));
        assert_eq!(doc.classes()[0].name().as_str(), "Renamed");
        assert_eq!(doc.classes()[1].name().as_str(), "B");
    }

    #[test]
    fn deleting_the_selected_class_selects_the_next_one() {
        let (mut doc, ids) = doc_with(&["A", "B", "C", "D"]);
        doc.select(ids[1]);
        doc.delete_class(ids[1]);
        assert_eq!(selected_name(&doc), Some("C"));
        doc.assert_invariants();
    }

    #[test]
    fn deleting_the_selected_last_class_selects_the_previous_one() {
        let (mut doc, ids) = doc_with(&["A", "B", "C"]);
        doc.delete_class(ids[2]);
        assert_eq!(selected_name(&doc), Some("B"));
    }

    #[test]
    fn deleting_the_only_class_leaves_nothing_selected() {
        let (mut doc, ids) = doc_with(&["A"]);
        doc.delete_class(ids[0]);
        assert!(doc.classes().is_empty());
        assert!(doc.selected().is_none());
        doc.assert_invariants();
    }

    #[test]
    fn deleting_another_class_keeps_the_selection() {
        let (mut doc, ids) = doc_with(&["A", "B", "C"]);
        doc.select(ids[2]);
        doc.delete_class(ids[0]);
        assert_eq!(selected_name(&doc), Some("C"));
    }

    #[test]
    fn deleting_is_idempotent() {
        let (mut doc, ids) = doc_with(&["A", "B"]);
        doc.delete_class(ids[0]);
        let once = doc.clone();
        doc.delete_class(ids[0]);
        assert_eq!(doc, once);
    }

    #[test]
    fn class_ids_are_not_reused_after_a_delete() {
        let (mut doc, ids) = doc_with(&["A", "B"]);
        doc.delete_class(ids[1]);
        let fresh = doc.create_class(label("C"), vec![]);
        assert!(!ids.contains(&fresh));
        doc.assert_invariants();
    }

    #[test]
    fn reconcile_repairs_selection_and_counters() {
        let (mut doc, ids) = doc_with(&["A", "B"]);
        doc.selected = Some(ClassId(500));
        doc.next_class = 0;
        doc.reconcile();
        assert_eq!(doc.selected().unwrap().id(), ids[0]);
        doc.assert_invariants();

        doc.selected = None;
        doc.reconcile();
        assert_eq!(doc.selected().unwrap().id(), ids[0]);

        let fresh = doc.create_class(label("C"), vec![]);
        assert!(!ids.contains(&fresh));
    }

    #[test]
    fn reconcile_clears_a_selection_when_no_class_exists() {
        let mut doc = Doc {
            classes: vec![],
            selected: Some(ClassId(3)),
            next_class: 0,
        };
        doc.reconcile();
        assert_eq!(doc.selected, None);
        doc.assert_invariants();
    }

    #[test]
    fn reconcile_is_idempotent() {
        let (mut doc, _) = doc_with(&["A", "B"]);
        doc.selected = Some(ClassId(500));
        doc.reconcile();
        let once = doc.clone();
        doc.reconcile();
        assert_eq!(doc, once);
    }

    #[test]
    fn reconcile_gives_a_duplicated_class_id_a_fresh_id() {
        let (mut doc, ids) = doc_with(&["A", "B"]);
        doc.classes[1].set_id(ids[0]);
        doc.reconcile();
        assert_eq!(doc.classes().len(), 2);
        assert_ne!(doc.classes()[0].id(), doc.classes()[1].id());
        doc.assert_invariants();
    }
}
