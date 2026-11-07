//! Input routing state shared by the event loop and the render loop.
//!
//! The event loop runs on its own task and decides, per key, whether a key is
//! looked up in the binds or sent to a text input as a plain character. That
//! decision depends on state the render loop owns (which input box is open, the
//! bind mode). One [`InputState`] per picker carries it between the two, instead
//! of process-wide statics that leaked between pickers and tests.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// Shared handle to a picker's [`InputState`].
pub type SharedInputState = Arc<InputState>;

#[derive(Debug, Default)]
pub struct InputState {
    /// Bind mode: binds whose trigger names this mode win over mode-less ones.
    mode: Mutex<String>,
    /// The action box (fm rename/create, ...) takes typed characters.
    action_box: AtomicBool,
    /// The navigation-mode prompt input takes typed characters.
    prompt_input: AtomicBool,
    /// A live input (query typed into a running command) takes typed characters.
    live_input: AtomicBool,
    /// In navigation mode, the filter has keyboard focus (else the results do).
    input_focused: AtomicBool,
    /// What the render loop showed after its last batch; see [`RenderView`].
    preview_fullscreen: AtomicBool,
    preview_diagram: AtomicBool,
    box_takes_keys: AtomicBool,
    sort_menu: AtomicBool,
    /// The last [`RenderCommand::Fence`](crate::message::RenderCommand::Fence) the render loop
    /// has applied, published with the view it left.
    fence_done: AtomicU64,
    fence_notify: tokio::sync::Notify,
}

/// Render-loop state that decides how the event loop resolves a navigation-mode key.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RenderView {
    pub preview_fullscreen: bool,
    pub preview_diagram: bool,
    /// An input box (create, rename, ...) or an input overlay takes the keys.
    pub box_takes_keys: bool,
    /// The sort menu is open: the next key picks the order.
    pub sort_menu: bool,
}

impl InputState {
    pub fn shared() -> SharedInputState {
        Arc::new(Self::default())
    }

    pub fn mode(&self) -> String {
        self.mode.lock().map(|m| m.clone()).unwrap_or_default()
    }

    pub fn set_mode(&self, mode: String) {
        if let Ok(mut m) = self.mode.lock() {
            *m = mode;
        }
    }

    pub fn action_box_active(&self) -> bool {
        self.action_box.load(Ordering::Relaxed)
    }

    pub fn set_action_box_active(&self, active: bool) {
        self.action_box.store(active, Ordering::Relaxed);
    }

    pub fn prompt_input_active(&self) -> bool {
        self.prompt_input.load(Ordering::Relaxed)
    }

    pub fn set_prompt_input_active(&self, active: bool) {
        self.prompt_input.store(active, Ordering::Relaxed);
    }

    pub fn live_input_active(&self) -> bool {
        self.live_input.load(Ordering::Relaxed)
    }

    pub fn set_live_input_active(&self, active: bool) {
        self.live_input.store(active, Ordering::Relaxed);
    }

    /// Which pane has keyboard focus in navigation mode (the results by default).
    pub fn focus(&self) -> crate::render::Focus {
        if self.input_focused.load(Ordering::Relaxed) {
            crate::render::Focus::Input
        } else {
            crate::render::Focus::Results
        }
    }

    pub fn set_focus(&self, focus: crate::render::Focus) {
        self.input_focused
            .store(focus == crate::render::Focus::Input, Ordering::Relaxed);
    }

    /// The view the render loop published last.
    pub fn view(&self) -> RenderView {
        RenderView {
            preview_fullscreen: self.preview_fullscreen.load(Ordering::Relaxed),
            preview_diagram: self.preview_diagram.load(Ordering::Relaxed),
            box_takes_keys: self.box_takes_keys.load(Ordering::Relaxed),
            sort_menu: self.sort_menu.load(Ordering::Relaxed),
        }
    }

    /// Called by the render loop between batches: everything up to fence `fence` is applied and
    /// left this view.
    pub fn publish(&self, view: RenderView, fence: u64) {
        self.preview_fullscreen
            .store(view.preview_fullscreen, Ordering::Relaxed);
        self.preview_diagram
            .store(view.preview_diagram, Ordering::Relaxed);
        self.box_takes_keys
            .store(view.box_takes_keys, Ordering::Relaxed);
        self.sort_menu.store(view.sort_menu, Ordering::Relaxed);
        // Release: a reader that sees this fence also sees the view and the focus above.
        if self.fence_done.fetch_max(fence, Ordering::Release) < fence {
            self.fence_notify.notify_waiters();
        }
    }

    pub fn fence_done(&self) -> u64 {
        self.fence_done.load(Ordering::Acquire)
    }

    /// Waits until the render loop has applied fence `fence`, or `limit` has passed. Returns
    /// whether it caught up.
    pub async fn render_caught_up(&self, fence: u64, limit: std::time::Duration) -> bool {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let notified = self.fence_notify.notified();
            tokio::pin!(notified);
            // Register before checking, so a publish between the check and the wait is seen.
            notified.as_mut().enable();
            if self.fence_done() >= fence {
                return true;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return self.fence_done() >= fence;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_pickers_do_not_share_flags() {
        let a = InputState::shared();
        let b = InputState::shared();
        a.set_mode("t0".into());
        a.set_action_box_active(true);
        a.set_prompt_input_active(true);
        a.set_live_input_active(true);
        assert_eq!(b.mode(), "");
        assert!(!b.action_box_active());
        assert!(!b.prompt_input_active());
        assert!(!b.live_input_active());

        let a2 = Arc::clone(&a);
        assert_eq!(a2.mode(), "t0");
        assert!(a2.action_box_active() && a2.prompt_input_active() && a2.live_input_active());
    }

    #[test]
    fn focus_starts_on_the_results_and_is_shared() {
        use crate::render::Focus;
        let a = InputState::shared();
        assert_eq!(a.focus(), Focus::Results);
        let b = Arc::clone(&a);
        b.set_focus(Focus::Input);
        assert_eq!(a.focus(), Focus::Input);
        a.set_focus(Focus::Results);
        assert_eq!(b.focus(), Focus::Results);
        assert_eq!(InputState::shared().focus(), Focus::Results);
    }

    #[test]
    fn publish_shares_the_view_and_keeps_the_highest_fence() {
        let a = InputState::shared();
        let view = RenderView {
            preview_fullscreen: true,
            preview_diagram: false,
            box_takes_keys: true,
            sort_menu: true,
        };
        a.publish(view, 3);
        assert_eq!((a.view(), a.fence_done()), (view, 3));
        // A fence the render loop dropped (while waiting for a pause ack) cannot move it back.
        a.publish(RenderView::default(), 2);
        assert_eq!((a.view(), a.fence_done()), (RenderView::default(), 3));
    }

    #[tokio::test]
    async fn waiting_for_the_render_loop() {
        use std::time::Duration;
        let a = InputState::shared();
        a.publish(RenderView::default(), 1);
        assert!(a.render_caught_up(1, Duration::ZERO).await);
        assert!(!a.render_caught_up(2, Duration::from_millis(10)).await);

        let b = Arc::clone(&a);
        let publisher = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            b.publish(RenderView::default(), 2);
        });
        assert!(a.render_caught_up(2, Duration::from_secs(5)).await);
        publisher.await.unwrap();
    }
}
