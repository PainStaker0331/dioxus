/*
Welcome to Dioxus's cooperative, priority-based scheduler.

I hope you enjoy your stay.

Some essential reading:
- https://github.com/facebook/react/blob/main/packages/scheduler/src/forks/Scheduler.js#L197-L200
- https://github.com/facebook/react/blob/main/packages/scheduler/src/forks/Scheduler.js#L440
- https://github.com/WICG/is-input-pending
- https://web.dev/rail/
- https://indepth.dev/posts/1008/inside-fiber-in-depth-overview-of-the-new-reconciliation-algorithm-in-react

# What's going on?

Dioxus is a framework for "user experience" - not just "user interfaces." Part of the "experience" is keeping the UI
snappy and "jank free" even under heavy work loads. Dioxus already has the "speed" part figured out - but there's no
point in being "fast" if you can't also be "responsive."

As such, Dioxus can manually decide on what work is most important at any given moment in time. With a properly tuned
priority system, Dioxus can ensure that user interaction is prioritized and committed as soon as possible (sub 100ms).
The controller responsible for this priority management is called the "scheduler" and is responsible for juggling many
different types of work simultaneously.

# How does it work?

Per the RAIL guide, we want to make sure that A) inputs are handled ASAP and B) animations are not blocked.
React-three-fiber is a testament to how amazing this can be - a ThreeJS scene is threaded in between work periods of
React, and the UI still stays snappy!

While it's straightforward to run code ASAP and be as "fast as possible", what's not  _not_ straightforward is how to do
this while not blocking the main thread. The current prevailing thought is to stop working periodically so the browser
has time to paint and run animations. When the browser is finished, we can step in and continue our work.

React-Fiber uses the "Fiber" concept to achieve a pause-resume functionality. This is worth reading up on, but not
necessary to understand what we're doing here. In Dioxus, our DiffMachine is guided by DiffInstructions - essentially
"commands" that guide the Diffing algorithm through the tree. Our "diff_scope" method is async - we can literally pause
our DiffMachine "mid-sentence" (so to speak) by just stopping the poll on the future. The DiffMachine periodically yields
so Rust's async machinery can take over, allowing us to customize when exactly to pause it.

React's "should_yield" method is more complex than ours, and I assume we'll move in that direction as Dioxus matures. For
now, Dioxus just assumes a TimeoutFuture, and selects! on both the Diff algorithm and timeout. If the DiffMachine finishes
before the timeout, then Dioxus will work on any pending work in the interim. If there is no pending work, then the changes
are committed, and coroutines are polled during the idle period. However, if the timeout expires, then the DiffMachine
future is paused and saved (self-referentially).

# Priorty System

So far, we've been able to thread our Dioxus work between animation frames - the main thread is not blocked! But that
doesn't help us _under load_. How do we still stay snappy... even if we're doing a lot of work? Well, that's where
priorities come into play. The goal with priorities is to schedule shorter work as a "high" priority and longer work as
a "lower" priority. That way, we can interrupt long-running low-prioty work with short-running high-priority work.

React's priority system is quite complex.

There are 5 levels of priority and 2 distinctions between UI events (discrete, continuous). I believe React really only
uses 3 priority levels and "idle" priority isn't used... Regardless, there's some batching going on.

For Dioxus, we're going with a 4 tier priorty system:
- Sync: Things that need to be done by the next frame, like TextInput on controlled elements
- High: for events that block all others - clicks, keyboard, and hovers
- Medium: for UI events caused by the user but not directly - scrolls/forms/focus (all other events)
- Low: set_state called asynchronously, and anything generated by suspense

In "Sync" state, we abort our "idle wait" future, and resolve the sync queue immediately and escape. Because we completed
work before the next rAF, any edits can be immediately processed before the frame ends. Generally though, we want to leave
as much time to rAF as possible. "Sync" is currently only used by onInput - we'll leave some docs telling people not to
do anything too arduous from onInput.

For the rest, we defer to the rIC period and work down each queue from high to low.
*/
use crate::heuristics::*;
use crate::innerlude::*;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures_util::stream::FuturesUnordered;
use futures_util::{future::FusedFuture, pin_mut, Future, FutureExt, StreamExt};
use fxhash::{FxHashMap, FxHashSet};
use indexmap::IndexSet;
use slab::Slab;
use smallvec::SmallVec;
use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell, RefMut, UnsafeCell},
    collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque},
    fmt::Display,
    pin::Pin,
    rc::Rc,
};

#[derive(Clone)]
pub(crate) struct EventChannel {
    pub task_counter: Rc<Cell<u64>>,
    pub sender: UnboundedSender<SchedulerMsg>,
    pub schedule_any_immediate: Rc<dyn Fn(ScopeId)>,
    pub submit_task: Rc<dyn Fn(FiberTask) -> TaskHandle>,
    pub get_shared_context: Rc<dyn Fn(ScopeId, TypeId) -> Option<Rc<dyn Any>>>,
}

pub enum SchedulerMsg {
    // events from the host
    UiEvent(UserEvent),

    // setstate
    Immediate(ScopeId),

    // tasks
    SubmitTask(FiberTask, u64),
    ToggleTask(u64),
    PauseTask(u64),
    ResumeTask(u64),
    DropTask(u64),
}

/// The scheduler holds basically everything around "working"
///
/// Each scope has the ability to lightly interact with the scheduler (IE, schedule an update) but ultimately the scheduler calls the components.
///
/// In Dioxus, the scheduler provides 4 priority levels - each with their own "DiffMachine". The DiffMachine state can be saved if the deadline runs
/// out.
///
/// Saved DiffMachine state can be self-referential, so we need to be careful about how we save it. All self-referential data is a link between
/// pending DiffInstructions, Mutations, and their underlying Scope. It's okay for us to be self-referential with this data, provided we don't priority
/// task shift to a higher priority task that needs mutable access to the same scopes.
///
/// We can prevent this safety issue from occurring if we track which scopes are invalidated when starting a new task.
///
///
pub(crate) struct Scheduler {
    /// All mounted components are arena allocated to make additions, removals, and references easy to work with
    /// A generational arena is used to re-use slots of deleted scopes without having to resize the underlying arena.
    ///
    /// This is wrapped in an UnsafeCell because we will need to get mutable access to unique values in unique bump arenas
    /// and rusts's guartnees cannot prove that this is safe. We will need to maintain the safety guarantees manually.
    pub pool: ResourcePool,

    pub heuristics: HeuristicsEngine,

    pub receiver: UnboundedReceiver<SchedulerMsg>,

    // Garbage stored
    pub pending_garbage: FxHashSet<ScopeId>,

    // In-flight futures
    pub async_tasks: FuturesUnordered<FiberTask>,

    // scheduler stuff
    pub current_priority: EventPriority,

    pub ui_events: VecDeque<UserEvent>,

    pub pending_immediates: VecDeque<ScopeId>,

    pub pending_tasks: VecDeque<UserEvent>,

    pub garbage_scopes: HashSet<ScopeId>,

    pub lanes: [PriorityLane; 4],
}

impl Scheduler {
    pub fn new() -> Self {
        /*
        Preallocate 2000 elements and 100 scopes to avoid dynamic allocation.
        Perhaps this should be configurable from some external config?
        */
        let components = Rc::new(UnsafeCell::new(Slab::with_capacity(100)));
        let raw_elements = Rc::new(UnsafeCell::new(Slab::with_capacity(2000)));

        let heuristics = HeuristicsEngine::new();

        let (sender, receiver) = futures_channel::mpsc::unbounded::<SchedulerMsg>();
        let task_counter = Rc::new(Cell::new(0));

        let channel = EventChannel {
            task_counter: task_counter.clone(),
            sender: sender.clone(),
            schedule_any_immediate: {
                let sender = sender.clone();
                Rc::new(move |id| sender.unbounded_send(SchedulerMsg::Immediate(id)).unwrap())
            },
            submit_task: {
                let sender = sender.clone();
                Rc::new(move |fiber_task| {
                    let task_id = task_counter.get();
                    task_counter.set(task_id + 1);
                    sender
                        .unbounded_send(SchedulerMsg::SubmitTask(fiber_task, task_id))
                        .unwrap();
                    TaskHandle {
                        our_id: task_id,
                        sender: sender.clone(),
                    }
                })
            },
            get_shared_context: {
                let components = components.clone();
                Rc::new(move |id, ty| {
                    let components = unsafe { &*components.get() };
                    let mut search: Option<&Scope> = components.get(id.0);
                    while let Some(inner) = search.take() {
                        if let Some(shared) = inner.shared_contexts.borrow().get(&ty) {
                            return Some(shared.clone());
                        } else {
                            search = inner.parent_idx.map(|id| components.get(id.0)).flatten();
                        }
                    }
                    None
                })
            },
        };

        let pool = ResourcePool {
            components: components.clone(),
            raw_elements,
            channel,
        };

        Self {
            pool,

            receiver,

            async_tasks: FuturesUnordered::new(),

            pending_garbage: FxHashSet::default(),

            heuristics,

            ui_events: VecDeque::new(),

            pending_immediates: VecDeque::new(),

            pending_tasks: VecDeque::new(),

            garbage_scopes: HashSet::new(),

            current_priority: EventPriority::Low,

            // sorted high to low by priority (0 = immediate, 3 = low)
            lanes: [
                PriorityLane::new(),
                PriorityLane::new(),
                PriorityLane::new(),
                PriorityLane::new(),
            ],
        }
    }

    pub fn manually_poll_events(&mut self) {
        while let Ok(Some(msg)) = self.receiver.try_next() {
            self.handle_channel_msg(msg);
        }
    }

    // Converts UI events into dirty scopes with various priorities
    pub fn consume_pending_events(&mut self) {
        // consume all events that are "continuous" to be batched
        // if we run into a discrete event, then bail early

        while let Some(trigger) = self.ui_events.pop_back() {
            if let Some(scope) = self.pool.get_scope_mut(trigger.scope) {
                if let Some(element) = trigger.mounted_dom_id {
                    let priority = match trigger.name {
                        // clipboard
                        "copy" | "cut" | "paste" => EventPriority::Medium,

                        // Composition
                        "compositionend" | "compositionstart" | "compositionupdate" => {
                            EventPriority::Low
                        }

                        // Keyboard
                        "keydown" | "keypress" | "keyup" => EventPriority::Low,

                        // Focus
                        "focus" | "blur" => EventPriority::Low,

                        // Form
                        "change" | "input" | "invalid" | "reset" | "submit" => EventPriority::Low,

                        // Mouse
                        "click" | "contextmenu" | "doubleclick" | "drag" | "dragend"
                        | "dragenter" | "dragexit" | "dragleave" | "dragover" | "dragstart"
                        | "drop" | "mousedown" | "mouseenter" | "mouseleave" | "mousemove"
                        | "mouseout" | "mouseover" | "mouseup" => EventPriority::Low,

                        // Pointer
                        "pointerdown" | "pointermove" | "pointerup" | "pointercancel"
                        | "gotpointercapture" | "lostpointercapture" | "pointerenter"
                        | "pointerleave" | "pointerover" | "pointerout" => EventPriority::Low,

                        // Selection
                        "select" | "touchcancel" | "touchend" => EventPriority::Low,

                        // Touch
                        "touchmove" | "touchstart" => EventPriority::Low,

                        // Wheel
                        "scroll" | "wheel" => EventPriority::Low,

                        // Media
                        "abort" | "canplay" | "canplaythrough" | "durationchange" | "emptied"
                        | "encrypted" | "ended" | "error" | "loadeddata" | "loadedmetadata"
                        | "loadstart" | "pause" | "play" | "playing" | "progress"
                        | "ratechange" | "seeked" | "seeking" | "stalled" | "suspend"
                        | "timeupdate" | "volumechange" | "waiting" => EventPriority::Low,

                        // Animation
                        "animationstart" | "animationend" | "animationiteration" => {
                            EventPriority::Low
                        }

                        // Transition
                        "transitionend" => EventPriority::Low,

                        // Toggle
                        "toggle" => EventPriority::Low,

                        _ => EventPriority::Low,
                    };

                    scope.call_listener(trigger.event, element);
                    // let receiver = self.immediate_receiver.clone();
                    // let mut receiver = receiver.borrow_mut();

                    // // Drain the immediates into the dirty scopes, setting the appropiate priorities
                    // while let Ok(Some(dirty_scope)) = receiver.try_next() {
                    //     self.add_dirty_scope(dirty_scope, trigger.priority)
                    // }
                }
            }
        }
    }

    // nothing to do, no events on channels, no work
    pub fn has_any_work(&self) -> bool {
        let pending_lanes = self.lanes.iter().find(|f| f.has_work()).is_some();
        pending_lanes || self.has_pending_events()
    }

    pub fn has_pending_events(&self) -> bool {
        self.ui_events.len() > 0
    }

    fn shift_priorities(&mut self) {
        self.current_priority = match (
            self.lanes[0].has_work(),
            self.lanes[1].has_work(),
            self.lanes[2].has_work(),
            self.lanes[3].has_work(),
        ) {
            (true, _, _, _) => EventPriority::Immediate,
            (false, true, _, _) => EventPriority::High,
            (false, false, true, _) => EventPriority::Medium,
            (false, false, false, _) => EventPriority::Low,
        };
    }

    /// re-balance the work lanes, ensuring high-priority work properly bumps away low priority work
    fn balance_lanes(&mut self) {}

    fn load_current_lane(&mut self) -> &mut PriorityLane {
        match self.current_priority {
            EventPriority::Immediate => todo!(),
            EventPriority::High => todo!(),
            EventPriority::Medium => todo!(),
            EventPriority::Low => todo!(),
        }
    }

    fn save_work(&mut self, lane: SavedDiffWork) {
        let saved: SavedDiffWork<'static> = unsafe { std::mem::transmute(lane) };
        self.load_current_lane().saved_state = Some(saved);
    }

    fn load_work(&mut self) -> SavedDiffWork<'static> {
        match self.current_priority {
            EventPriority::Immediate => todo!(),
            EventPriority::High => todo!(),
            EventPriority::Medium => todo!(),
            EventPriority::Low => todo!(),
        }
    }

    /// Work the scheduler down, not polling any ongoing tasks.
    ///
    /// Will use the standard priority-based scheduling, batching, etc, but just won't interact with the async reactor.
    pub fn work_sync<'a>(&'a mut self) -> Vec<Mutations<'a>> {
        let mut committed_mutations = Vec::new();

        self.manually_poll_events();

        if !self.has_any_work() {
            return committed_mutations;
        }

        self.consume_pending_events();

        while self.has_any_work() {
            self.shift_priorities();
            self.work_on_current_lane(|| false, &mut committed_mutations);
        }

        committed_mutations
    }

    /// The primary workhorse of the VirtualDOM.
    ///
    /// Uses some fairly complex logic to schedule what work should be produced.
    ///
    /// Returns a list of successful mutations.
    pub async fn work_with_deadline<'a>(
        &'a mut self,
        mut deadline_reached: Pin<Box<impl FusedFuture<Output = ()>>>,
    ) -> Vec<Mutations<'a>> {
        /*
        Strategy:
        - When called, check for any UI events that might've been received since the last frame.
        - Dump all UI events into a "pending discrete" queue and a "pending continuous" queue.

        - If there are any pending discrete events, then elevate our priorty level. If our priority level is already "high,"
            then we need to finish the high priority work first. If the current work is "low" then analyze what scopes
            will be invalidated by this new work. If this interferes with any in-flight medium or low work, then we need
            to bump the other work out of the way, or choose to process it so we don't have any conflicts.
            'static components have a leg up here since their work can be re-used among multiple scopes.
            "High priority" is only for blocking! Should only be used on "clicks"

        - If there are no pending discrete events, then check for continuous events. These can be completely batched


        Open questions:
        - what if we get two clicks from the component during the same slice?
            - should we batch?
            - react says no - they are continuous
            - but if we received both - then we don't need to diff, do we? run as many as we can and then finally diff?
        */
        let mut committed_mutations = Vec::<Mutations<'static>>::new();

        loop {
            // Internalize any pending work since the last time we ran
            self.manually_poll_events();

            // Wait for any new events if we have nothing to do
            if !self.has_any_work() {
                let deadline_expired = self.wait_for_any_trigger(&mut deadline_reached).await;

                if deadline_expired {
                    return committed_mutations;
                }
            }

            // Create work from the pending event queue
            self.consume_pending_events();

            // shift to the correct lane
            self.shift_priorities();

            let mut deadline_reached = || (&mut deadline_reached).now_or_never().is_some();

            let finished_before_deadline =
                self.work_on_current_lane(&mut deadline_reached, &mut committed_mutations);

            if !finished_before_deadline {
                break;
            }
        }

        committed_mutations
    }

    /// Load the current lane, and work on it, periodically checking in if the deadline has been reached.
    ///
    /// Returns true if the lane is finished before the deadline could be met.
    pub fn work_on_current_lane(
        &mut self,
        deadline_reached: impl FnMut() -> bool,
        mutations: &mut Vec<Mutations>,
    ) -> bool {
        // Work through the current subtree, and commit the results when it finishes
        // When the deadline expires, give back the work
        let saved_state = self.load_work();

        // We have to split away some parts of ourself - current lane is borrowed mutably
        let mut shared = self.pool.clone();
        let mut machine = unsafe { saved_state.promote(&mut shared) };

        if machine.stack.is_empty() {
            let shared = self.pool.clone();

            self.current_lane().dirty_scopes.sort_by(|a, b| {
                let h1 = shared.get_scope(*a).unwrap().height;
                let h2 = shared.get_scope(*b).unwrap().height;
                h1.cmp(&h2)
            });

            if let Some(scope) = self.current_lane().dirty_scopes.pop() {
                let component = self.pool.get_scope(scope).unwrap();
                let (old, new) = (component.frames.wip_head(), component.frames.fin_head());
                machine.stack.push(DiffInstruction::Diff { new, old });
            }
        }

        let deadline_expired = machine.work(deadline_reached);

        let machine: DiffMachine<'static> = unsafe { std::mem::transmute(machine) };
        let mut saved = machine.save();

        if deadline_expired {
            self.save_work(saved);
            false
        } else {
            for node in saved.seen_scopes.drain() {
                self.current_lane().dirty_scopes.remove(&node);
            }

            let mut new_mutations = Mutations::new();
            std::mem::swap(&mut new_mutations, &mut saved.mutations);

            mutations.push(new_mutations);
            self.save_work(saved);
            true
        }
    }

    // waits for a trigger, canceling early if the deadline is reached
    // returns true if the deadline was reached
    // does not return the trigger, but caches it in the scheduler
    pub async fn wait_for_any_trigger(
        &mut self,
        deadline: &mut Pin<Box<impl FusedFuture<Output = ()>>>,
    ) -> bool {
        use futures_util::future::{select, Either};

        let event_fut = async {
            match select(self.receiver.next(), self.async_tasks.next()).await {
                Either::Left((msg, _other)) => {
                    self.handle_channel_msg(msg.unwrap());
                }
                Either::Right((task, _other)) => {
                    // do nothing, async task will likely generate a set of scheduler messages
                }
            }
        };

        pin_mut!(event_fut);

        match select(event_fut, deadline).await {
            Either::Left((msg, _other)) => false,
            Either::Right((deadline, _)) => true,
        }
    }

    pub fn current_lane(&mut self) -> &mut PriorityLane {
        match self.current_priority {
            EventPriority::Immediate => &mut self.lanes[0],
            EventPriority::High => &mut self.lanes[1],
            EventPriority::Medium => &mut self.lanes[2],
            EventPriority::Low => &mut self.lanes[3],
        }
    }

    pub fn handle_channel_msg(&mut self, msg: SchedulerMsg) {
        match msg {
            SchedulerMsg::Immediate(_) => todo!(),
            SchedulerMsg::UiEvent(_) => todo!(),

            //
            SchedulerMsg::SubmitTask(_, _) => todo!(),
            SchedulerMsg::ToggleTask(_) => todo!(),
            SchedulerMsg::PauseTask(_) => todo!(),
            SchedulerMsg::ResumeTask(_) => todo!(),
            SchedulerMsg::DropTask(_) => todo!(),
        }
    }

    fn add_dirty_scope(&mut self, scope: ScopeId, priority: EventPriority) {
        todo!()
        // match priority {
        //     EventPriority::High => self.high_priorty.dirty_scopes.insert(scope),
        //     EventPriority::Medium => self.medium_priority.dirty_scopes.insert(scope),
        //     EventPriority::Low => self.low_priority.dirty_scopes.insert(scope),
        // };
    }

    fn collect_garbage(&mut self, id: ElementId) {
        //
    }
}

pub(crate) struct PriorityLane {
    pub dirty_scopes: IndexSet<ScopeId>,
    pub saved_state: Option<SavedDiffWork<'static>>,
    pub in_progress: bool,
}

impl PriorityLane {
    pub fn new() -> Self {
        Self {
            saved_state: None,
            dirty_scopes: Default::default(),
            in_progress: false,
        }
    }

    fn has_work(&self) -> bool {
        todo!()
    }

    fn work(&mut self) {
        let scope = self.dirty_scopes.pop();
    }
}

pub struct TaskHandle {
    pub(crate) sender: UnboundedSender<SchedulerMsg>,
    pub(crate) our_id: u64,
}

impl TaskHandle {
    /// Toggles this coroutine off/on.
    ///
    /// This method is not synchronous - your task will not stop immediately.
    pub fn toggle(&self) {
        self.sender
            .unbounded_send(SchedulerMsg::ToggleTask(self.our_id))
            .unwrap()
    }

    /// This method is not synchronous - your task will not stop immediately.
    pub fn resume(&self) {
        self.sender
            .unbounded_send(SchedulerMsg::ResumeTask(self.our_id))
            .unwrap()
    }

    /// This method is not synchronous - your task will not stop immediately.
    pub fn stop(&self) {
        self.sender
            .unbounded_send(SchedulerMsg::ToggleTask(self.our_id))
            .unwrap()
    }

    /// This method is not synchronous - your task will not stop immediately.
    pub fn restart(&self) {
        self.sender
            .unbounded_send(SchedulerMsg::ToggleTask(self.our_id))
            .unwrap()
    }
}

#[derive(serde::Serialize, serde::Deserialize, Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ScopeId(pub usize);

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ElementId(pub usize);
impl Display for ElementId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ElementId {
    pub fn as_u64(self) -> u64 {
        self.0 as u64
    }
}

/// Priority of Event Triggers.
///
/// Internally, Dioxus will abort work that's taking too long if new, more important, work arrives. Unlike React, Dioxus
/// won't be afraid to pause work or flush changes to the RealDOM. This is called "cooperative scheduling". Some Renderers
/// implement this form of scheduling internally, however Dioxus will perform its own scheduling as well.
///
/// The ultimate goal of the scheduler is to manage latency of changes, prioritizing "flashier" changes over "subtler" changes.
///
/// React has a 5-tier priority system. However, they break things into "Continuous" and "Discrete" priority. For now,
/// we keep it simple, and just use a 3-tier priority system.
///
/// - NoPriority = 0
/// - LowPriority = 1
/// - NormalPriority = 2
/// - UserBlocking = 3
/// - HighPriority = 4
/// - ImmediatePriority = 5
///
/// We still have a concept of discrete vs continuous though - discrete events won't be batched, but continuous events will.
/// This means that multiple "scroll" events will be processed in a single frame, but multiple "click" events will be
/// flushed before proceeding. Multiple discrete events is highly unlikely, though.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, PartialOrd, Ord)]
pub enum EventPriority {
    /// Work that must be completed during the EventHandler phase.
    ///
    /// Currently this is reserved for controlled inputs.
    Immediate = 3,

    /// "High Priority" work will not interrupt other high priority work, but will interrupt medium and low priority work.
    ///
    /// This is typically reserved for things like user interaction.
    ///
    /// React calls these "discrete" events, but with an extra category of "user-blocking" (Immediate).
    High = 2,

    /// "Medium priority" work is generated by page events not triggered by the user. These types of events are less important
    /// than "High Priority" events and will take presedence over low priority events.
    ///
    /// This is typically reserved for VirtualEvents that are not related to keyboard or mouse input.
    ///
    /// React calls these "continuous" events (e.g. mouse move, mouse wheel, touch move, etc).
    Medium = 1,

    /// "Low Priority" work will always be pre-empted unless the work is significantly delayed, in which case it will be
    /// advanced to the front of the work queue until completed.
    ///
    /// The primary user of Low Priority work is the asynchronous work system (suspense).
    ///
    /// This is considered "idle" work or "background" work.
    Low = 0,
}

#[derive(Clone)]
pub(crate) struct ResourcePool {
    /*
    This *has* to be an UnsafeCell.

    Each BumpFrame and Scope is located in this Slab - and we'll need mutable access to a scope while holding on to
    its bumpframe conents immutably.

    However, all of the interaction with this Slab is done in this module and the Diff module, so it should be fairly
    simple to audit.

    Wrapped in Rc so the "get_shared_context" closure can walk the tree (immutably!)
    */
    pub components: Rc<UnsafeCell<Slab<Scope>>>,

    /*
    Yes, a slab of "nil". We use this for properly ordering ElementIDs - all we care about is the allocation strategy
    that slab uses. The slab essentially just provides keys for ElementIDs that we can re-use in a Vec on the client.

    This just happened to be the simplest and most efficient way to implement a deterministic keyed map with slot reuse.

    In the future, we could actually store a pointer to the VNode instead of nil to provide O(1) lookup for VNodes...
    */
    pub raw_elements: Rc<UnsafeCell<Slab<()>>>,

    pub channel: EventChannel,
}

impl ResourcePool {
    /// this is unsafe because the caller needs to track which other scopes it's already using
    pub fn get_scope(&self, idx: ScopeId) -> Option<&Scope> {
        let inner = unsafe { &*self.components.get() };
        inner.get(idx.0)
    }

    /// this is unsafe because the caller needs to track which other scopes it's already using
    pub fn get_scope_mut(&self, idx: ScopeId) -> Option<&mut Scope> {
        let inner = unsafe { &mut *self.components.get() };
        inner.get_mut(idx.0)
    }

    pub fn with_scope<'b, O: 'static>(
        &'b self,
        _id: ScopeId,
        _f: impl FnOnce(&'b mut Scope) -> O,
    ) -> Option<O> {
        todo!()
    }

    // return a bumpframe with a lifetime attached to the arena borrow
    // this is useful for merging lifetimes
    pub fn with_scope_vnode<'b>(
        &self,
        _id: ScopeId,
        _f: impl FnOnce(&mut Scope) -> &VNode<'b>,
    ) -> Option<&VNode<'b>> {
        todo!()
    }

    pub fn try_remove(&self, id: ScopeId) -> Option<Scope> {
        let inner = unsafe { &mut *self.components.get() };
        Some(inner.remove(id.0))
        // .try_remove(id.0)
        // .ok_or_else(|| Error::FatalInternal("Scope not found"))
    }

    pub fn reserve_node(&self) -> ElementId {
        let els = unsafe { &mut *self.raw_elements.get() };
        ElementId(els.insert(()))
    }

    /// return the id, freeing the space of the original node
    pub fn collect_garbage(&self, id: ElementId) {
        todo!("garabge collection currently WIP")
        // self.raw_elements.remove(id.0);
    }

    pub fn insert_scope_with_key(&self, f: impl FnOnce(ScopeId) -> Scope) -> ScopeId {
        let g = unsafe { &mut *self.components.get() };
        let entry = g.vacant_entry();
        let id = ScopeId(entry.key());
        entry.insert(f(id));
        id
    }

    pub fn borrow_bumpframe(&self) {}
}
