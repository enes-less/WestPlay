//! The page's changes to the game's GPS route (Maps' search: go to a place, add a stop, end the route),
//! passed to the game plugin through the bridge ([`RouteBlock`]), and its answer passed back to the page.
//! One at a time: a request still unanswered when the next comes is overtaken, and the page told so.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use westplay_shared::{RouteAction, RouteAnswer, RouteBlock};

/// How long the plugin has to answer (it does at the end of the game's next frame), before the page is told
/// there was no answer (no game, or the game is loading).
const ANSWER_WITHIN: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct RouteChanges {
    pending: Option<Pending>,
}

struct Pending {
    /// The bridge's request number, and the page's for the same request.
    request: u32,
    id: u32,
    sent: Instant,
}

impl RouteChanges {
    /// Asks the plugin to change the route; the page's request `id` comes back with the answer. Returns the
    /// answer for an earlier request this overtakes, for the page.
    pub fn ask(
        &mut self,
        block: &RouteBlock,
        id: u32,
        action: RouteAction,
        node: u64,
        vias: &[u64],
        now: Instant,
    ) -> Option<String> {
        let overtaken = self.pending.take().map(|pending| answer_message(pending.id, None));
        block.action.store(action as u32, Ordering::Relaxed);
        block.node.store(node, Ordering::Relaxed);
        let count = vias.len().min(block.vias.len());
        for (slot, &via) in block.vias.iter().zip(&vias[..count]) {
            slot.store(via, Ordering::Relaxed);
        }
        block.via_count.store(count as u32, Ordering::Relaxed);
        let request = block.request.load(Ordering::Relaxed).wrapping_add(1);
        block.request.store(request, Ordering::Release);
        self.pending = Some(Pending { request, id, sent: now });
        overtaken
    }

    /// The plugin's answer to the request waiting, once it has answered (or not in time), for the page.
    pub fn answer(&mut self, block: &RouteBlock, now: Instant) -> Option<String> {
        let pending = self.pending.as_ref()?;
        let message = if block.answered.load(Ordering::Acquire) == pending.request {
            answer_message(pending.id, RouteAnswer::from_code(block.answer.load(Ordering::Relaxed)))
        } else if now.duration_since(pending.sent) >= ANSWER_WITHIN {
            answer_message(pending.id, None)
        } else {
            return None;
        };
        self.pending = None;
        Some(message)
    }
}

/// `{"type":"routeAnswer","id":7,"answer":"done"}`: how the game answered the page's request `id`;
/// `"noAnswer"` when it didn't (no game, a request overtaken, an answer WestPlay doesn't know).
fn answer_message(id: u32, answer: Option<RouteAnswer>) -> String {
    let answer = match answer {
        Some(RouteAnswer::Done) => "done",
        Some(RouteAnswer::AlreadyThere) => "alreadyThere",
        Some(RouteAnswer::OnJob) => "onJob",
        Some(RouteAnswer::Full) => "full",
        Some(RouteAnswer::NoSuchNode) => "noSuchNode",
        Some(RouteAnswer::Unsupported) => "unsupported",
        Some(RouteAnswer::Unexpected) => "unexpected",
        None => "noAnswer",
    };
    serde_json::json!({ "type": "routeAnswer", "id": id, "answer": answer }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64};

    fn block() -> RouteBlock {
        RouteBlock {
            action: AtomicU32::new(0),
            node: AtomicU64::new(0),
            request: AtomicU32::new(41),
            answered: AtomicU32::new(41),
            answer: AtomicI32::new(0),
            vias: Default::default(),
            via_count: AtomicU32::new(0),
        }
    }

    /// The plugin's part: answers the request waiting.
    fn plugin_answers(block: &RouteBlock, answer: RouteAnswer) {
        block.answer.store(answer as i32, Ordering::Relaxed);
        block.answered.store(block.request.load(Ordering::Relaxed), Ordering::Release);
    }

    #[test]
    fn asks_the_plugin_and_passes_its_answer_to_the_page() {
        let (block, mut changes, now) = (block(), RouteChanges::default(), Instant::now());
        assert_eq!(changes.ask(&block, 7, RouteAction::AddStop, 0x478e_f4c4_cd16_0002, &[], now), None);
        assert_eq!(block.action.load(Ordering::Relaxed), RouteAction::AddStop as u32);
        assert_eq!(block.node.load(Ordering::Relaxed), 0x478e_f4c4_cd16_0002);
        assert_eq!(block.request.load(Ordering::Relaxed), 42);
        // Not answered yet.
        assert_eq!(changes.answer(&block, now), None);
        plugin_answers(&block, RouteAnswer::OnJob);
        assert_eq!(changes.answer(&block, now).as_deref(), Some(r#"{"answer":"onJob","id":7,"type":"routeAnswer"}"#));
        // Passed on once.
        assert_eq!(changes.answer(&block, now), None);
    }

    #[test]
    fn tells_the_page_when_no_answer_comes_or_a_request_is_overtaken() {
        let (block, mut changes, now) = (block(), RouteChanges::default(), Instant::now());
        changes.ask(&block, 1, RouteAction::Go, 5, &[], now);
        assert_eq!(changes.answer(&block, now + ANSWER_WITHIN / 2), None);
        let late = changes.answer(&block, now + ANSWER_WITHIN);
        assert_eq!(late.as_deref(), Some(r#"{"answer":"noAnswer","id":1,"type":"routeAnswer"}"#));
        changes.ask(&block, 2, RouteAction::Go, 5, &[], now);
        let overtaken = changes.ask(&block, 3, RouteAction::End, 0, &[], now);
        assert_eq!(overtaken.as_deref(), Some(r#"{"answer":"noAnswer","id":2,"type":"routeAnswer"}"#));
        plugin_answers(&block, RouteAnswer::Done);
        assert_eq!(changes.answer(&block, now).as_deref(), Some(r#"{"answer":"done","id":3,"type":"routeAnswer"}"#));
    }
}
