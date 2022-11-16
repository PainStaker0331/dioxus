use super::{waker::RcWake, SchedulerMsg};
use crate::ElementId;
use crate::{innerlude::Mutations, Element, ScopeId};
use std::future::Future;
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
};

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SuspenseId(pub usize);

pub type SuspenseContext = Rc<SuspenseBoundary>;

/// Essentially a fiber in React
pub struct SuspenseBoundary {
    pub id: ScopeId,
    pub waiting_on: RefCell<HashSet<SuspenseId>>,
    pub mutations: RefCell<Mutations<'static>>,
    pub placeholder: Cell<Option<ElementId>>,
}

impl SuspenseBoundary {
    pub fn new(id: ScopeId) -> Rc<Self> {
        Rc::new(Self {
            id,
            waiting_on: Default::default(),
            mutations: RefCell::new(Mutations::new(0)),
            placeholder: Cell::new(None),
        })
    }
}

pub(crate) struct SuspenseLeaf {
    pub id: SuspenseId,
    pub scope_id: ScopeId,
    pub tx: futures_channel::mpsc::UnboundedSender<SchedulerMsg>,
    pub notified: Cell<bool>,
    pub task: *mut dyn Future<Output = Element<'static>>,
}

impl RcWake for SuspenseLeaf {
    fn wake_by_ref(arc_self: &Rc<Self>) {
        arc_self.notified.set(true);
        _ = arc_self
            .tx
            .unbounded_send(SchedulerMsg::SuspenseNotified(arc_self.id));
    }
}
