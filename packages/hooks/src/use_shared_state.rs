use self::error::{UseSharedStateError, UseSharedStateResult};
use dioxus_core::{ScopeId, ScopeState};
use std::{
    cell::{Ref, RefCell, RefMut},
    collections::HashSet,
    rc::Rc,
    sync::Arc,
};

#[cfg(not(debug_assertions))]
type Location = ();

#[cfg(debug_assertions)]
type Location = &'static std::panic::Location<'static>;

#[macro_export]
macro_rules! debug_location {
    () => {{
        #[cfg(debug_assertions)]
        {
            std::panic::Location::caller()
        }
        #[cfg(not(debug_assertions))]
        {
            ()
        }
    }};
}

pub mod diagnostics {
    use std::panic::Location;

    #[derive(Debug, Clone, Copy)]
    pub enum BorrowKind {
        Mutable,
        Immutable,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct PreviousBorrow {
        pub location: Location<'static>,
        pub kind: BorrowKind,
    }

    impl<T> super::UseSharedState<T> {
        #[cfg_attr(debug_assertions, track_caller)]
        #[cfg_attr(debug_assertions, inline(never))]
        #[allow(unused_must_use)]
        pub(super) fn debug_track_borrow(&self) {
            #[cfg(debug_assertions)]
            self.previous_borrow
                .borrow_mut()
                .insert(PreviousBorrow::borrowed());
        }

        #[cfg_attr(debug_assertions, track_caller)]
        #[cfg_attr(debug_assertions, inline(never))]
        #[allow(unused_must_use)]
        pub(super) fn debug_track_borrow_mut(&self) {
            #[cfg(debug_assertions)]
            self.previous_borrow
                .borrow_mut()
                .insert(PreviousBorrow::borrowed_mut());
        }
    }
    impl PreviousBorrow {
        #[track_caller]
        #[inline(never)]
        pub fn borrowed() -> Self {
            Self {
                location: *Location::caller(),
                kind: BorrowKind::Immutable,
            }
        }

        #[track_caller]
        #[inline(never)]
        pub fn borrowed_mut() -> Self {
            Self {
                location: *Location::caller(),
                kind: BorrowKind::Mutable,
            }
        }
    }
}

pub mod error {
    #[derive(thiserror::Error, Debug)]
    pub enum UseSharedStateError {
        #[cfg_attr(
            debug_assertions,
            error(
            "[{location}] {type_name} is already borrowed at [{previous_borrow:?}], so it cannot be borrowed mutably."
            )
         )]
        #[cfg_attr(
            not(debug_assertions),
            error("{type_name} is already borrowed, so it cannot be borrowed mutably.")
        )]
        AlreadyBorrowed {
            source: core::cell::BorrowMutError,
            type_name: &'static str,
            /// Only available in debug mode
            location: super::Location,
            #[cfg(debug_assertions)]
            previous_borrow: Option<super::diagnostics::PreviousBorrow>,
        },
        #[cfg_attr(
            debug_assertions,
            error(
            "[{location}] {type_name} is already borrowed mutably at [{previous_borrow:?}], so it cannot be borrowed anymore."
            )
         )]
        #[cfg_attr(
            not(debug_assertions),
            error("{type_name} is already borrowed mutably, so it cannot be borrowed anymore.")
        )]
        AlreadyBorrowedMutably {
            source: core::cell::BorrowError,
            type_name: &'static str,
            /// Only available in debug mode
            location: super::Location,
            #[cfg(debug_assertions)]
            previous_borrow: Option<super::diagnostics::PreviousBorrow>,
        },
    }

    pub type UseSharedStateResult<T> = Result<T, UseSharedStateError>;
}

type ProvidedState<T> = Rc<RefCell<ProvidedStateInner<T>>>;

// Tracks all the subscribers to a shared State
pub(crate) struct ProvidedStateInner<T> {
    value: T,
    notify_any: Arc<dyn Fn(ScopeId)>,
    consumers: HashSet<ScopeId>,
}

impl<T> ProvidedStateInner<T> {
    pub(crate) fn notify_consumers(&mut self) {
        for consumer in self.consumers.iter() {
            (self.notify_any)(*consumer);
        }
    }
}

/// This hook provides some relatively light ergonomics around shared state.
///
/// It is not a substitute for a proper state management system, but it is capable enough to provide use_state - type
/// ergonomics in a pinch, with zero cost.
///
/// # Example
///
/// ```rust
/// # use dioxus::prelude::*;
/// #
/// # fn app(cx: Scope) -> Element {
/// #     render! {
/// #         Parent{}
/// #     }
/// # }
///
/// #[derive(Clone, Copy)]
/// enum Theme {
///     Light,
///     Dark,
/// }
///
/// // Provider
/// fn Parent<'a>(cx: Scope<'a>) -> Element<'a> {
///     use_shared_state_provider(cx, || Theme::Dark);
///     let theme = use_shared_state::<Theme>(cx).unwrap();
///
///     render! {
///         button{
///             onclick: move |_| {
///                 let current_theme = *theme.read();
///                 *theme.write() = match current_theme {
///                     Theme::Dark => Theme::Light,
///                     Theme::Light => Theme::Dark,
///                 };
///             },
///             "Change theme"
///         }
///         Child{}
///     }
/// }
///
/// // Consumer
/// fn Child<'a>(cx: Scope<'a>) -> Element<'a> {
///     let theme = use_shared_state::<Theme>(cx).unwrap();
///     let current_theme = *theme.read();
///
///     render! {
///         match current_theme {
///             Theme::Dark => {
///                 "Dark mode"
///             }
///             Theme::Light => {
///                 "Light mode"
///             }
///         }
///     }
/// }
/// ```
///
/// # How it works
///
/// Any time a component calls `write`, every consumer of the state will be notified - excluding the provider.
///
/// Right now, there is not a distinction between read-only and write-only, so every consumer will be notified.
pub fn use_shared_state<T: 'static>(cx: &ScopeState) -> Option<&UseSharedState<T>> {
    let state: &Option<UseSharedStateOwner<T>> = &*cx.use_hook(move || {
        let scope_id = cx.scope_id();
        let root = cx.consume_context::<ProvidedState<T>>()?;

        root.borrow_mut().consumers.insert(scope_id);

        let state = UseSharedState::new(root);
        let owner = UseSharedStateOwner { state, scope_id };
        Some(owner)
    });
    state.as_ref().map(|s| &s.state)
}

/// This wrapper detects when the hook is dropped and will unsubscribe when the component is unmounted
struct UseSharedStateOwner<T> {
    state: UseSharedState<T>,
    scope_id: ScopeId,
}

impl<T> Drop for UseSharedStateOwner<T> {
    fn drop(&mut self) {
        // we need to unsubscribe when our component is unmounted
        let mut root = self.state.inner.borrow_mut();
        root.consumers.remove(&self.scope_id);
    }
}

/// State that is shared between components through the context system
pub struct UseSharedState<T> {
    pub(crate) inner: Rc<RefCell<ProvidedStateInner<T>>>,
    #[cfg(debug_assertions)]
    previous_borrow: Rc<RefCell<Option<diagnostics::PreviousBorrow>>>,
}

impl<T> UseSharedState<T> {
    fn new(inner: Rc<RefCell<ProvidedStateInner<T>>>) -> Self {
        Self {
            inner,
            #[cfg(debug_assertions)]
            previous_borrow: Default::default(),
        }
    }

    /// Notify all consumers of the state that it has changed. (This is called automatically when you call "write")
    pub fn notify_consumers(&self) {
        self.inner.borrow_mut().notify_consumers();
    }

    /// Try reading the shared state
    #[cfg_attr(debug_assertions, track_caller)]
    #[cfg_attr(debug_assertions, inline(never))]
    pub fn try_read(&self) -> UseSharedStateResult<Ref<'_, T>> {
        match self.inner.try_borrow() {
            Ok(value) => {
                self.debug_track_borrow();
                Ok(Ref::map(value, |inner| &inner.value))
            }
            Err(source) => Err(UseSharedStateError::AlreadyBorrowedMutably {
                source,
                type_name: std::any::type_name::<Self>(),
                location: debug_location!(),
                #[cfg(debug_assertions)]
                previous_borrow: *self.previous_borrow.borrow(),
            }),
        }
    }

    /// Read the shared value
    #[cfg_attr(debug_assertions, track_caller)]
    #[cfg_attr(debug_assertions, inline(never))]
    pub fn read(&self) -> Ref<'_, T> {
        match self.try_read() {
            Ok(value) => value,
            Err(message) => panic!(
                "Reading the shared state failed: {}\n({:?})",
                message, message
            ),
        }
    }

    /// Try writing the shared state
    #[cfg_attr(debug_assertions, track_caller)]
    #[cfg_attr(debug_assertions, inline(never))]
    pub fn try_write(&self) -> UseSharedStateResult<RefMut<'_, T>> {
        match self.inner.try_borrow_mut() {
            Ok(mut value) => {
                self.debug_track_borrow_mut();
                value.notify_consumers();
                Ok(RefMut::map(value, |inner| &mut inner.value))
            }
            Err(source) => Err(UseSharedStateError::AlreadyBorrowed {
                source,
                type_name: std::any::type_name::<Self>(),
                location: crate::debug_location!(),
                #[cfg(debug_assertions)]
                previous_borrow: *self.previous_borrow.borrow(),
            }),
        }
    }

    /// Calling "write" will force the component to re-render
    ///
    ///
    // TODO: We prevent unncessary notifications only in the hook, but we should figure out some more global lock
    #[cfg_attr(debug_assertions, track_caller)]
    #[cfg_attr(debug_assertions, inline(never))]
    pub fn write(&self) -> RefMut<'_, T> {
        match self.try_write() {
            Ok(value) => value,
            Err(message) => panic!(
                "Writing to shared state failed: {}\n({:?})",
                message, message
            ),
        }
    }

    /// Tries writing the value without forcing a re-render
    #[cfg_attr(debug_assertions, track_caller)]
    #[cfg_attr(debug_assertions, inline(never))]
    pub fn try_write_silent(&self) -> UseSharedStateResult<RefMut<'_, T>> {
        match self.inner.try_borrow_mut() {
            Ok(value) => {
                self.debug_track_borrow_mut();
                Ok(RefMut::map(value, |inner| &mut inner.value))
            }
            Err(source) => Err(UseSharedStateError::AlreadyBorrowed {
                source,
                type_name: std::any::type_name::<Self>(),
                location: crate::debug_location!(),
                #[cfg(debug_assertions)]
                previous_borrow: *self.previous_borrow.borrow(),
            }),
        }
    }

    /// Writes the value without forcing a re-render
    #[cfg_attr(debug_assertions, track_caller)]
    #[cfg_attr(debug_assertions, inline(never))]
    pub fn write_silent(&self) -> RefMut<'_, T> {
        match self.try_write_silent() {
            Ok(value) => value,
            Err(message) => panic!(
                "Writing to shared state silently failed: {}\n({:?})",
                message, message
            ),
        }
    }
}

impl<T> Clone for UseSharedState<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            #[cfg(debug_assertions)]
            previous_borrow: self.previous_borrow.clone(),
        }
    }
}

impl<T: PartialEq> PartialEq for UseSharedState<T> {
    fn eq(&self, other: &Self) -> bool {
        let first = self.inner.borrow();
        let second = other.inner.borrow();
        first.value == second.value
    }
}

/// Provide some state for components down the hierarchy to consume without having to drill props. See [`use_shared_state`] to consume the state
///
///
/// # Example
///
/// ```rust
/// # use dioxus::prelude::*;
/// #
/// # fn app(cx: Scope) -> Element {
/// #     render! {
/// #         Parent{}
/// #     }
/// # }
///
/// #[derive(Clone, Copy)]
/// enum Theme {
///     Light,
///     Dark,
/// }
///
/// // Provider
/// fn Parent<'a>(cx: Scope<'a>) -> Element<'a> {
///     use_shared_state_provider(cx, || Theme::Dark);
///     let theme = use_shared_state::<Theme>(cx).unwrap();
///
///     render! {
///         button{
///             onclick: move |_| {
///                 let current_theme = *theme.read();
///                 *theme.write() = match current_theme {
///                     Theme::Dark => Theme::Light,
///                     Theme::Light => Theme::Dark,
///                 };
///             },
///             "Change theme"
///         }
///         // Children components that consume the state...
///     }
/// }
/// ```
pub fn use_shared_state_provider<T: 'static>(cx: &ScopeState, f: impl FnOnce() -> T) {
    cx.use_hook(|| {
        let state: ProvidedState<T> = Rc::new(RefCell::new(ProvidedStateInner {
            value: f(),
            notify_any: cx.schedule_update_any(),
            consumers: HashSet::new(),
        }));

        cx.provide_context(state);
    });
}
