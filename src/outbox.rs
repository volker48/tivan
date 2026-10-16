//! Orders the writes of the doc to the server. A teacher's edits arrive in bursts, so a write
//! waits until they pause, at most one request is on the wire, and edits made while it flies
//! are folded into one later request with the newest doc. The shell owns the clock and the
//! network and asks this what to do, which keeps the ordering testable without either.

/// Identifies one edit, so the timer set for it can tell whether a newer edit has taken over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edit(u64);

#[derive(Debug, Default)]
pub struct Outbox {
    /// What the server is known to hold.
    synced: Option<String>,
    /// The newest doc, whether or not it has been sent.
    latest: Option<String>,
    /// The envelope on the wire right now.
    sending: Option<String>,
    edits: u64,
    /// The newest edit has paused long enough to be sent.
    due: bool,
    halted: bool,
}

impl Outbox {
    /// Records what the server holds, so a doc identical to it is never sent back.
    pub fn mark_synced(&mut self, envelope: Option<String>) {
        self.synced = envelope;
    }

    /// The doc changed to `envelope`. The caller waits out the debounce and then calls
    /// [`Outbox::settled`] with the returned edit.
    pub fn edited(&mut self, envelope: String) -> Edit {
        self.edits += 1;
        self.due = false;
        if !self.halted {
            self.latest = Some(envelope);
        }
        Edit(self.edits)
    }

    /// The debounce for `edit` is over. Returns the envelope to send now, unless a newer edit has
    /// replaced this one, a request is still in flight, or the server already holds this doc. A
    /// request in flight picks the edit up when it finishes.
    pub fn settled(&mut self, edit: Edit) -> Option<String> {
        if edit.0 != self.edits {
            return None;
        }
        self.due = true;
        self.start()
    }

    /// Treats the newest edit as settled without waiting out its debounce, for when the page is
    /// about to be left. Returns the envelope to send, under the same rules as
    /// [`Outbox::settled`]. A request already in flight picks the edit up when it finishes.
    pub fn settle_now(&mut self) -> Option<String> {
        self.due = true;
        self.start()
    }

    /// A request is on the wire.
    pub fn in_flight(&self) -> bool {
        self.sending.is_some()
    }

    /// The newest doc is not on the server, whether it is waiting, in flight, or was refused.
    pub fn has_unsent(&self) -> bool {
        self.latest.is_some() && self.latest != self.synced
    }

    /// The request returned. Returns the next envelope to send if edits settled while it flew.
    /// A failed request is not tried again until a newer edit settles, so an outage does not
    /// become a loop.
    pub fn finished(&mut self, saved: bool) -> Option<String> {
        let sent = self.sending.take();
        // A request that failed or timed out may still have been committed, so the server's
        // copy is unknown and the next settled edit must be sent even if it equals the old one.
        self.synced = if saved { sent } else { None };
        self.start()
    }

    /// Stops all writing for good, for when the session has ended.
    pub fn halt(&mut self) {
        self.halted = true;
        self.latest = None;
        self.sending = None;
    }

    fn start(&mut self) -> Option<String> {
        if self.halted || !self.due || self.sending.is_some() {
            return None;
        }
        self.due = false;
        let envelope = self
            .latest
            .as_ref()
            .filter(|latest| self.synced.as_ref() != Some(latest))?
            .clone();
        self.sending = Some(envelope.clone());
        Some(envelope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(raw: &str) -> String {
        raw.to_owned()
    }

    /// Edits to `envelope` and lets its debounce pass.
    fn settle(outbox: &mut Outbox, envelope: &str) -> Option<String> {
        let edit = outbox.edited(text(envelope));
        outbox.settled(edit)
    }

    #[test]
    fn a_single_edit_is_sent_once_its_debounce_is_over() {
        let mut outbox = Outbox::default();
        let edit = outbox.edited(text("a"));

        assert_eq!(outbox.settled(edit), Some(text("a")));
        assert_eq!(outbox.finished(true), None);
    }

    #[test]
    fn a_burst_of_edits_sends_only_the_last() {
        let mut outbox = Outbox::default();
        let first = outbox.edited(text("a"));
        let second = outbox.edited(text("b"));
        let third = outbox.edited(text("c"));

        assert_eq!(outbox.settled(first), None);
        assert_eq!(outbox.settled(second), None);
        assert_eq!(outbox.settled(third), Some(text("c")));
        assert_eq!(outbox.finished(true), None);
    }

    #[test]
    fn edits_during_a_request_wait_for_it_and_are_folded_into_one_request() {
        let mut outbox = Outbox::default();
        assert_eq!(settle(&mut outbox, "a"), Some(text("a")));

        let b = outbox.edited(text("b"));
        let c = outbox.edited(text("c"));
        assert_eq!(outbox.settled(b), None);
        assert_eq!(
            outbox.settled(c),
            None,
            "one request at a time, even when the debounce is over"
        );

        assert_eq!(outbox.finished(true), Some(text("c")));
        assert_eq!(outbox.finished(true), None);
    }

    #[test]
    fn a_request_in_flight_ignores_edits_that_are_still_debouncing() {
        let mut outbox = Outbox::default();
        assert_eq!(settle(&mut outbox, "a"), Some(text("a")));
        outbox.edited(text("b"));

        assert_eq!(
            outbox.finished(true),
            None,
            "the newer edit is still waiting out its debounce"
        );
    }

    #[test]
    fn a_doc_the_server_already_holds_is_not_sent() {
        let mut outbox = Outbox::default();
        outbox.mark_synced(Some(text("a")));

        assert_eq!(settle(&mut outbox, "a"), None);
        assert_eq!(settle(&mut outbox, "b"), Some(text("b")));
    }

    #[test]
    fn an_edit_back_to_what_the_server_holds_cancels_the_pending_write() {
        let mut outbox = Outbox::default();
        outbox.mark_synced(Some(text("a")));
        let changed = outbox.edited(text("b"));
        let reverted = outbox.edited(text("a"));

        assert_eq!(outbox.settled(changed), None);
        assert_eq!(outbox.settled(reverted), None);
    }

    #[test]
    fn an_edit_back_to_the_old_doc_while_a_request_flies_is_still_sent_afterwards() {
        let mut outbox = Outbox::default();
        outbox.mark_synced(Some(text("old")));
        assert_eq!(settle(&mut outbox, "new"), Some(text("new")));

        assert_eq!(settle(&mut outbox, "old"), None);

        assert_eq!(
            outbox.finished(true),
            Some(text("old")),
            "the server must end up with the newest doc"
        );
    }

    #[test]
    fn the_server_is_not_sent_what_it_just_received() {
        let mut outbox = Outbox::default();
        assert_eq!(settle(&mut outbox, "a"), Some(text("a")));
        assert_eq!(settle(&mut outbox, "a"), None);

        assert_eq!(outbox.finished(true), None);
    }

    #[test]
    fn a_failed_request_is_not_retried_until_the_next_edit() {
        let mut outbox = Outbox::default();
        assert_eq!(settle(&mut outbox, "a"), Some(text("a")));

        assert_eq!(outbox.finished(false), None);

        assert_eq!(
            settle(&mut outbox, "a"),
            Some(text("a")),
            "the server never got it, so the next edit sends it again"
        );
    }

    #[test]
    fn after_a_failed_request_an_edit_back_to_the_old_synced_doc_is_still_sent() {
        let mut outbox = Outbox::default();
        outbox.mark_synced(Some(text("old")));
        assert_eq!(settle(&mut outbox, "new"), Some(text("new")));
        assert_eq!(outbox.finished(false), None, "it may have committed anyway");

        assert_eq!(
            settle(&mut outbox, "old"),
            Some(text("old")),
            "the server may hold `new`, so `old` has to be sent to settle it"
        );
    }

    #[test]
    fn a_newer_edit_that_settled_during_a_failed_request_is_sent_straight_away() {
        let mut outbox = Outbox::default();
        assert_eq!(settle(&mut outbox, "a"), Some(text("a")));
        assert_eq!(settle(&mut outbox, "b"), None);

        assert_eq!(outbox.finished(false), Some(text("b")));
    }

    #[test]
    fn leaving_the_page_sends_the_edit_without_waiting_for_its_debounce() {
        let mut outbox = Outbox::default();
        let edit = outbox.edited(text("a"));

        assert!(outbox.has_unsent());
        assert_eq!(outbox.settle_now(), Some(text("a")));
        assert!(outbox.in_flight());
        assert_eq!(outbox.finished(true), None);
        assert!(!outbox.in_flight());
        assert!(!outbox.has_unsent());
        assert_eq!(outbox.settled(edit), None, "its timer finds nothing left");
    }

    #[test]
    fn leaving_during_a_request_has_the_request_send_what_is_newer_when_it_returns() {
        let mut outbox = Outbox::default();
        assert_eq!(settle(&mut outbox, "a"), Some(text("a")));
        outbox.edited(text("b"));

        assert_eq!(outbox.settle_now(), None);
        assert!(outbox.in_flight());
        assert_eq!(outbox.finished(true), Some(text("b")));
        assert_eq!(outbox.finished(true), None);
        assert!(!outbox.has_unsent());
    }

    #[test]
    fn a_refused_doc_stays_unsent_so_leaving_can_be_stopped() {
        let mut outbox = Outbox::default();
        outbox.mark_synced(Some(text("old")));
        assert!(!outbox.has_unsent());

        assert_eq!(settle(&mut outbox, "new"), Some(text("new")));
        assert_eq!(outbox.finished(false), None);

        assert!(outbox.has_unsent());
        assert!(!outbox.in_flight());
    }

    #[test]
    fn nothing_is_sent_after_the_session_ends() {
        let mut outbox = Outbox::default();
        assert_eq!(settle(&mut outbox, "a"), Some(text("a")));
        let pending = outbox.edited(text("b"));
        outbox.halt();

        assert_eq!(outbox.settled(pending), None);
        assert_eq!(outbox.finished(false), None);
        assert_eq!(settle(&mut outbox, "c"), None);
    }
}
