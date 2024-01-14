use crate::{global_context::current_scope_id, runtime::with_runtime, ScopeId};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// A wrapper around some generic data that handles the event's state
///
///
/// Prevent this event from continuing to bubble up the tree to parent elements.
///
/// # Example
///
/// ```rust, ignore
/// rsx! {
///     button {
///         onclick: move |evt: Event<MouseData>| {
///             evt.cancel_bubble();
///
///         }
///     }
/// }
/// ```
pub struct Event<T: 'static + ?Sized> {
    /// The data associated with this event
    pub data: Rc<T>,
    pub(crate) propagates: Rc<Cell<bool>>,
}

impl<T> Event<T> {
    /// Map the event data to a new type
    ///
    /// # Example
    ///
    /// ```rust, ignore
    /// rsx! {
    ///    button {
    ///       onclick: move |evt: Event<FormData>| {
    ///          let data = evt.map(|data| data.value());
    ///          assert_eq!(data.inner(), "hello world");
    ///       }
    ///    }
    /// }
    /// ```
    pub fn map<U: 'static, F: FnOnce(&T) -> U>(&self, f: F) -> Event<U> {
        Event {
            data: Rc::new(f(&self.data)),
            propagates: self.propagates.clone(),
        }
    }

    /// Prevent this event from continuing to bubble up the tree to parent elements.
    ///
    /// # Example
    ///
    /// ```rust, ignore
    /// rsx! {
    ///     button {
    ///         onclick: move |evt: Event<MouseData>| {
    ///             evt.cancel_bubble();
    ///         }
    ///     }
    /// }
    /// ```
    #[deprecated = "use stop_propagation instead"]
    pub fn cancel_bubble(&self) {
        self.propagates.set(false);
    }

    /// Prevent this event from continuing to bubble up the tree to parent elements.
    ///
    /// # Example
    ///
    /// ```rust, ignore
    /// rsx! {
    ///     button {
    ///         onclick: move |evt: Event<MouseData>| {
    ///             evt.stop_propagation();
    ///         }
    ///     }
    /// }
    /// ```
    pub fn stop_propagation(&self) {
        self.propagates.set(false);
    }

    /// Get a reference to the inner data from this event
    ///
    /// ```rust, ignore
    /// rsx! {
    ///     button {
    ///         onclick: move |evt: Event<MouseData>| {
    ///             let data = evt.inner.clone();
    ///             cx.spawn(async move {
    ///                 println!("{:?}", data);
    ///             });
    ///         }
    ///     }
    /// }
    /// ```
    pub fn inner(&self) -> &Rc<T> {
        &self.data
    }
}

impl<T: ?Sized> Clone for Event<T> {
    fn clone(&self) -> Self {
        Self {
            propagates: self.propagates.clone(),
            data: self.data.clone(),
        }
    }
}

impl<T> std::ops::Deref for Event<T> {
    type Target = Rc<T>;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Event<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiEvent")
            .field("bubble_state", &self.propagates)
            .field("data", &self.data)
            .finish()
    }
}

/// The callback type generated by the `rsx!` macro when an `on` field is specified for components.
///
/// This makes it possible to pass `move |evt| {}` style closures into components as property fields.
///
///
/// # Example
///
/// ```rust, ignore
/// rsx!{
///     MyComponent { onclick: move |evt| tracing::debug!("clicked") }
/// }
///
/// #[derive(Props)]
/// struct MyProps<'a> {
///     onclick: EventHandler<'a, MouseEvent>,
/// }
///
/// fn MyComponent(cx: MyProps) -> Element {
///     rsx!{
///         button {
///             onclick: move |evt| cx.onclick.call(evt),
///         }
///     })
/// }
///
/// ```
pub struct EventHandler<T = ()> {
    pub(crate) origin: ScopeId,
    pub(super) callback: Rc<RefCell<Option<ExternalListenerCallback<T>>>>,
}

impl<T> Clone for EventHandler<T> {
    fn clone(&self) -> Self {
        Self {
            origin: self.origin,
            callback: self.callback.clone(),
        }
    }
}

impl<T> PartialEq for EventHandler<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.callback, &other.callback)
    }
}

impl<T> Default for EventHandler<T> {
    fn default() -> Self {
        Self {
            origin: ScopeId::ROOT,
            callback: Default::default(),
        }
    }
}

type ExternalListenerCallback<T> = Box<dyn FnMut(T)>;

impl<T> EventHandler<T> {
    /// Create a new [`EventHandler`] from an [`FnMut`]
    pub fn new(mut f: impl FnMut(T) + 'static) -> EventHandler<T> {
        let callback = Rc::new(RefCell::new(Some(Box::new(move |event: T| {
            f(event);
        }) as Box<dyn FnMut(T)>)));
        EventHandler {
            callback,
            origin: current_scope_id().expect("to be in a dioxus runtime"),
        }
    }

    /// Call this event handler with the appropriate event type
    ///
    /// This borrows the event using a RefCell. Recursively calling a listener will cause a panic.
    pub fn call(&self, event: T) {
        if let Some(callback) = self.callback.borrow_mut().as_mut() {
            with_runtime(|rt| {
                rt.scope_stack.borrow_mut().push(self.origin);
            });
            callback(event);
            with_runtime(|rt| {
                rt.scope_stack.borrow_mut().pop();
            });
        }
    }

    /// Forcibly drop the internal handler callback, releasing memory
    ///
    /// This will force any future calls to "call" to not doing anything
    pub fn release(&self) {
        self.callback.replace(None);
    }
}
