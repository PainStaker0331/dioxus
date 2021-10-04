//! An event system that's less confusing than Traits + RC;
//! This should hopefully make it easier to port to other platforms.
//!
//! Unfortunately, it is less efficient than the original, but hopefully it's negligible.

use crate::{
    innerlude::Listener,
    innerlude::{ElementId, NodeFactory, ScopeId},
};
use bumpalo::boxed::Box as BumpBox;
use std::{
    any::Any,
    cell::{Cell, RefCell},
    fmt::Debug,
    ops::Deref,
    rc::Rc,
    sync::Arc,
};

#[derive(Debug)]
pub struct UserEvent {
    /// The originator of the event trigger
    pub scope: ScopeId,

    /// The optional real node associated with the trigger
    pub mounted_dom_id: Option<ElementId>,

    /// The event type IE "onclick" or "onmouseover"
    ///
    /// The name that the renderer will use to mount the listener.
    pub name: &'static str,

    /// The type of event
    pub event: SyntheticEvent,
}

#[derive(Debug)]
pub enum SyntheticEvent {
    AnimationEvent(on::AnimationEvent),
    ClipboardEvent(on::ClipboardEvent),
    CompositionEvent(on::CompositionEvent),
    FocusEvent(on::FocusEvent),
    FormEvent(on::FormEvent),
    KeyboardEvent(on::KeyboardEvent),
    GenericEvent(DioxusEvent<()>),
    TouchEvent(on::TouchEvent),
    ToggleEvent(on::ToggleEvent),
    MediaEvent(on::MediaEvent),
    MouseEvent(on::MouseEvent),
    WheelEvent(on::WheelEvent),
    SelectionEvent(on::SelectionEvent),
    TransitionEvent(on::TransitionEvent),
    PointerEvent(on::PointerEvent),
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

#[derive(Debug)]
pub struct DioxusEvent<T: Send> {
    inner: T,
    raw: Box<dyn Any + Send>,
}

impl<T: Send + Sync> DioxusEvent<T> {
    pub fn new<F: Send + 'static>(inner: T, raw: F) -> Self {
        let raw = Box::new(raw);
        Self { inner, raw }
    }

    /// Return a reference to the raw event. User will need to downcast the event to the right platform-specific type.
    pub fn native<E: 'static>(&self) -> Option<&E> {
        self.raw.downcast_ref()
    }

    /// Returns whether or not a specific event is a bubbling event
    pub fn bubbles(&self) -> bool {
        todo!()
    }
    /// Sets or returns whether the event should propagate up the hierarchy or not
    pub fn cancel_bubble(&self) {
        todo!()
    }
    /// Returns whether or not an event can have its default action prevented
    pub fn cancelable(&self) -> bool {
        todo!()
    }
    /// Returns whether the event is composed or not
    pub fn composed(&self) -> bool {
        todo!()
    }

    // Currently not supported because those no way we could possibly support it
    // just cast the event to the right platform-specific type and return it
    // /// Returns the event's path
    // pub fn composed_path(&self) -> String {
    //     todo!()
    // }

    /// Returns the element whose event listeners triggered the event
    pub fn current_target(&self) {
        todo!()
    }
    /// Returns whether or not the preventDefault method was called for the event
    pub fn default_prevented(&self) -> bool {
        todo!()
    }
    /// Returns which phase of the event flow is currently being evaluated
    pub fn event_phase(&self) -> u16 {
        todo!()
    }

    /// Returns whether or not an event is trusted
    pub fn is_trusted(&self) -> bool {
        todo!()
    }

    /// Cancels the event if it is cancelable, meaning that the default action that belongs to the event will
    pub fn prevent_default(&self) {
        todo!()
    }

    /// Prevents other listeners of the same event from being called
    pub fn stop_immediate_propagation(&self) {
        todo!()
    }

    /// Prevents further propagation of an event during event flow
    pub fn stop_propagation(&self) {
        todo!()
    }

    /// Returns the element that triggered the event
    pub fn target(&self) -> Option<Box<dyn Any>> {
        todo!()
    }

    /// Returns the time (in milliseconds relative to the epoch) at which the event was created
    pub fn time_stamp(&self) -> f64 {
        todo!()
    }
}

impl<T: Send + Sync> std::ops::Deref for DioxusEvent<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub mod on {
    use super::*;
    macro_rules! event_directory {
        ( $(
            $( #[$attr:meta] )*
            $eventdata:ident($wrapper:ident): [
                $(
                    $( #[$method_attr:meta] )*
                    $name:ident
                )*
            ];
        )* ) => {
            $(
                $(#[$attr])*
                #[derive(Debug)]
                pub struct $wrapper(pub DioxusEvent<$eventdata>);

                // todo: derefing to the event is fine (and easy) but breaks some IDE stuff like (go to source)
                // going to source in fact takes you to the source of Rc which is... less than useful
                // Either we ask for this to be changed in Rust-analyzer or manually impkement the trait
                impl Deref for $wrapper {
                    type Target = DioxusEvent<$eventdata>;
                    fn deref(&self) -> &Self::Target {
                        &self.0
                    }
                }

                $(
                    $(#[$method_attr])*
                    pub fn $name<'a, F>(
                        c: NodeFactory<'a>,
                        mut callback: F,
                    ) -> Listener<'a>
                        where F: FnMut($wrapper) + 'a
                    {
                        let bump = &c.bump();

                        let cb: &mut dyn FnMut(SyntheticEvent) = bump.alloc(move |evt: SyntheticEvent| match evt {
                            SyntheticEvent::$wrapper(event) => callback(event),
                            _ => unreachable!("Downcasted SyntheticEvent to wrong event type - this is an internal bug!")
                        });

                        let callback: BumpBox<dyn FnMut(SyntheticEvent) + 'a> = unsafe { BumpBox::from_raw(cb) };


                        // ie oncopy
                        let event_name = stringify!($name);

                        // ie copy
                        let shortname: &'static str = &event_name[2..];

                        Listener {
                            event: shortname,
                            mounted_node: Cell::new(None),
                            callback: RefCell::new(Some(callback)),
                        }
                    }
                )*
            )*
        };
    }

    // The Dioxus Synthetic event system
    event_directory! {
        ClipboardEventInner(ClipboardEvent): [
            /// Called when "copy"
            oncopy

            /// oncut
            oncut

            /// onpaste
            onpaste
        ];

        CompositionEventInner(CompositionEvent): [
            /// oncompositionend
            oncompositionend

            /// oncompositionstart
            oncompositionstart

            /// oncompositionupdate
            oncompositionupdate
        ];

        KeyboardEventInner(KeyboardEvent): [
            /// onkeydown
            onkeydown

            /// onkeypress
            onkeypress

            /// onkeyup
            onkeyup
        ];

        FocusEventInner(FocusEvent): [
            /// onfocus
            onfocus

            /// onblur
            onblur
        ];


        FormEventInner(FormEvent): [
            /// onchange
            onchange

            /// oninput handler
            oninput

            /// oninvalid
            oninvalid

            /// onreset
            onreset

            /// onsubmit
            onsubmit
        ];


        /// A synthetic event that wraps a web-style [`MouseEvent`](https://developer.mozilla.org/en-US/docs/Web/API/MouseEvent)
        ///
        ///
        /// The MouseEvent interface represents events that occur due to the user interacting with a pointing device (such as a mouse).
        ///
        /// ## Trait implementation:
        /// ```rust
        ///     fn alt_key(&self) -> bool;
        ///     fn button(&self) -> i16;
        ///     fn buttons(&self) -> u16;
        ///     fn client_x(&self) -> i32;
        ///     fn client_y(&self) -> i32;
        ///     fn ctrl_key(&self) -> bool;
        ///     fn meta_key(&self) -> bool;
        ///     fn page_x(&self) -> i32;
        ///     fn page_y(&self) -> i32;
        ///     fn screen_x(&self) -> i32;
        ///     fn screen_y(&self) -> i32;
        ///     fn shift_key(&self) -> bool;
        ///     fn get_modifier_state(&self, key_code: &str) -> bool;
        /// ```
        ///
        /// ## Event Handlers
        /// - [`onclick`]
        /// - [`oncontextmenu`]
        /// - [`ondoubleclick`]
        /// - [`ondrag`]
        /// - [`ondragend`]
        /// - [`ondragenter`]
        /// - [`ondragexit`]
        /// - [`ondragleave`]
        /// - [`ondragover`]
        /// - [`ondragstart`]
        /// - [`ondrop`]
        /// - [`onmousedown`]
        /// - [`onmouseenter`]
        /// - [`onmouseleave`]
        /// - [`onmousemove`]
        /// - [`onmouseout`]
        /// - [`onmouseover`]
        /// - [`onmouseup`]
        MouseEventInner(MouseEvent): [
            /// Execute a callback when a button is clicked.
            ///
            /// ## Description
            ///
            /// An element receives a click event when a pointing device button (such as a mouse's primary mouse button)
            /// is both pressed and released while the pointer is located inside the element.
            ///
            /// - Bubbles: Yes
            /// - Cancelable: Yes
            /// - Interface: [`MouseEvent`]
            ///
            /// If the button is pressed on one element and the pointer is moved outside the element before the button
            /// is released, the event is fired on the most specific ancestor element that contained both elements.
            /// `click` fires after both the `mousedown` and `mouseup` events have fired, in that order.
            ///
            /// ## Example
            /// ```
            /// rsx!( button { "click me", onclick: move |_| log::info!("Clicked!`") } )
            /// ```
            ///
            /// ## Reference
            /// - https://www.w3schools.com/tags/ev_onclick.asp
            /// - https://developer.mozilla.org/en-US/docs/Web/API/Element/click_event
            onclick

            /// oncontextmenu
            oncontextmenu

            /// ondoubleclick
            ondoubleclick

            /// ondrag
            ondrag

            /// ondragend
            ondragend

            /// ondragenter
            ondragenter

            /// ondragexit
            ondragexit

            /// ondragleave
            ondragleave

            /// ondragover
            ondragover

            /// ondragstart
            ondragstart

            /// ondrop
            ondrop

            /// onmousedown
            onmousedown

            /// onmouseenter
            onmouseenter

            /// onmouseleave
            onmouseleave

            /// onmousemove
            onmousemove

            /// onmouseout
            onmouseout

            ///
            onscroll

            /// onmouseover
            ///
            /// Triggered when the users's mouse hovers over an element.
            onmouseover

            /// onmouseup
            onmouseup
        ];

        PointerEventInner(PointerEvent): [
            /// pointerdown
            onpointerdown

            /// pointermove
            onpointermove

            /// pointerup
            onpointerup

            /// pointercancel
            onpointercancel

            /// gotpointercapture
            ongotpointercapture

            /// lostpointercapture
            onlostpointercapture

            /// pointerenter
            onpointerenter

            /// pointerleave
            onpointerleave

            /// pointerover
            onpointerover

            /// pointerout
            onpointerout
        ];

        SelectionEventInner(SelectionEvent): [
            /// onselect
            onselect
        ];

        TouchEventInner(TouchEvent): [
            /// ontouchcancel
            ontouchcancel

            /// ontouchend
            ontouchend

            /// ontouchmove
            ontouchmove

            /// ontouchstart
            ontouchstart
        ];

        WheelEventInner(WheelEvent): [
            ///
            onwheel
        ];

        MediaEventInner(MediaEvent): [
            ///abort
            onabort

            ///canplay
            oncanplay

            ///canplaythrough
            oncanplaythrough

            ///durationchange
            ondurationchange

            ///emptied
            onemptied

            ///encrypted
            onencrypted

            ///ended
            onended

            ///error
            onerror

            ///loadeddata
            onloadeddata

            ///loadedmetadata
            onloadedmetadata

            ///loadstart
            onloadstart

            ///pause
            onpause

            ///play
            onplay

            ///playing
            onplaying

            ///progress
            onprogress

            ///ratechange
            onratechange

            ///seeked
            onseeked

            ///seeking
            onseeking

            ///stalled
            onstalled

            ///suspend
            onsuspend

            ///timeupdate
            ontimeupdate

            ///volumechange
            onvolumechange

            ///waiting
            onwaiting
        ];

        AnimationEventInner(AnimationEvent): [
            /// onanimationstart
            onanimationstart

            /// onanimationend
            onanimationend

            /// onanimationiteration
            onanimationiteration
        ];

        TransitionEventInner(TransitionEvent): [
            ///
            ontransitionend
        ];

        ToggleEventInner(ToggleEvent): [
            ///
            ontoggle
        ];
    }

    #[derive(Debug)]
    pub struct ClipboardEventInner(
        // DOMDataTransfer clipboardData
    );

    #[derive(Debug)]
    pub struct CompositionEventInner {
        pub data: String,
    }

    #[derive(Debug)]
    pub struct KeyboardEventInner {
        pub alt_key: bool,
        pub char_code: u32,

        /// Identify which "key" was entered.
        ///
        /// This is the best method to use for all languages. They key gets mapped to a String sequence which you can match on.
        /// The key isn't an enum because there are just so many context-dependent keys.
        ///
        /// A full list on which keys to use is available at:
        /// <https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/key/Key_Values>
        ///
        /// # Example
        ///
        /// ```rust
        /// match event.key().as_str() {
        ///     "Esc" | "Escape" => {}
        ///     "ArrowDown" => {}
        ///     "ArrowLeft" => {}
        ///      _ => {}
        /// }
        /// ```
        ///    
        pub key: String,

        /// Get the key code as an enum Variant.
        ///
        /// This is intended for things like arrow keys, escape keys, function keys, and other non-international keys.
        /// To match on unicode sequences, use the [`key`] method - this will return a string identifier instead of a limited enum.
        ///
        ///
        /// ## Example
        ///
        /// ```rust
        /// use dioxus::KeyCode;
        /// match event.key_code() {
        ///     KeyCode::Escape => {}
        ///     KeyCode::LeftArrow => {}
        ///     KeyCode::RightArrow => {}
        ///     _ => {}
        /// }
        /// ```
        ///    
        pub key_code: KeyCode,
        pub ctrl_key: bool,
        pub locale: String,
        pub location: usize,
        pub meta_key: bool,
        pub repeat: bool,
        pub shift_key: bool,
        pub which: usize,
        // get_modifier_state: bool,
    }

    #[derive(Debug)]
    pub struct FocusEventInner {/* DOMEventInner:  Send + SyncTarget relatedTarget */}

    #[derive(Debug)]
    pub struct FormEventInner {
        /* DOMEventInner:  Send + SyncTarget relatedTarget */
        pub value: String,
    }

    #[derive(Debug)]
    pub struct MouseEventInner {
        pub alt_key: bool,
        pub button: i16,
        pub buttons: u16,
        pub client_x: i32,
        pub client_y: i32,
        pub ctrl_key: bool,
        pub meta_key: bool,
        pub page_x: i32,
        pub page_y: i32,
        pub screen_x: i32,
        pub screen_y: i32,
        pub shift_key: bool,
        // fn get_modifier_state(&self, key_code: &str) -> bool;
    }

    #[derive(Debug)]
    pub struct PointerEventInner {
        // Mouse only
        pub alt_key: bool,
        pub button: i16,
        pub buttons: u16,
        pub client_x: i32,
        pub client_y: i32,
        pub ctrl_key: bool,
        pub meta_key: bool,
        pub page_x: i32,
        pub page_y: i32,
        pub screen_x: i32,
        pub screen_y: i32,
        pub shift_key: bool,
        pub pointer_id: i32,
        pub width: i32,
        pub height: i32,
        pub pressure: f32,
        pub tangential_pressure: f32,
        pub tilt_x: i32,
        pub tilt_y: i32,
        pub twist: i32,
        pub pointer_type: String,
        pub is_primary: bool,
        // pub get_modifier_state: bool,
    }

    #[derive(Debug)]
    pub struct SelectionEventInner {}

    #[derive(Debug)]
    pub struct TouchEventInner {
        pub alt_key: bool,
        pub ctrl_key: bool,
        pub meta_key: bool,
        pub shift_key: bool,
        // get_modifier_state: bool,
        // changedTouches: DOMTouchList,
        // targetTouches: DOMTouchList,
        // touches: DOMTouchList,
    }

    #[derive(Debug)]
    pub struct WheelEventInner {
        pub delta_mode: u32,
        pub delta_x: f64,
        pub delta_y: f64,
        pub delta_z: f64,
    }

    #[derive(Debug)]
    pub struct MediaEventInner {}

    #[derive(Debug)]
    pub struct ImageEventInner {
        //     load error
        pub load_error: bool,
    }

    #[derive(Debug)]
    pub struct AnimationEventInner {
        pub animation_name: String,
        pub pseudo_element: String,
        pub elapsed_time: f32,
    }

    #[derive(Debug)]
    pub struct TransitionEventInner {
        pub property_name: String,
        pub pseudo_element: String,
        pub elapsed_time: f32,
    }

    #[derive(Debug)]
    pub struct ToggleEventInner {}
}

#[derive(Clone, Copy, Debug)]
pub enum KeyCode {
    Backspace = 8,
    Tab = 9,
    Enter = 13,
    Shift = 16,
    Ctrl = 17,
    Alt = 18,
    Pause = 19,
    CapsLock = 20,
    Escape = 27,
    PageUp = 33,
    PageDown = 34,
    End = 35,
    Home = 36,
    LeftArrow = 37,
    UpArrow = 38,
    RightArrow = 39,
    DownArrow = 40,
    Insert = 45,
    Delete = 46,
    Num0 = 48,
    Num1 = 49,
    Num2 = 50,
    Num3 = 51,
    Num4 = 52,
    Num5 = 53,
    Num6 = 54,
    Num7 = 55,
    Num8 = 56,
    Num9 = 57,
    A = 65,
    B = 66,
    C = 67,
    D = 68,
    E = 69,
    F = 70,
    G = 71,
    H = 72,
    I = 73,
    J = 74,
    K = 75,
    L = 76,
    M = 77,
    N = 78,
    O = 79,
    P = 80,
    Q = 81,
    R = 82,
    S = 83,
    T = 84,
    U = 85,
    V = 86,
    W = 87,
    X = 88,
    Y = 89,
    Z = 90,
    LeftWindow = 91,
    RightWindow = 92,
    SelectKey = 93,
    Numpad0 = 96,
    Numpad1 = 97,
    Numpad2 = 98,
    Numpad3 = 99,
    Numpad4 = 100,
    Numpad5 = 101,
    Numpad6 = 102,
    Numpad7 = 103,
    Numpad8 = 104,
    Numpad9 = 105,
    Multiply = 106,
    Add = 107,
    Subtract = 109,
    DecimalPoint = 110,
    Divide = 111,
    F1 = 112,
    F2 = 113,
    F3 = 114,
    F4 = 115,
    F5 = 116,
    F6 = 117,
    F7 = 118,
    F8 = 119,
    F9 = 120,
    F10 = 121,
    F11 = 122,
    F12 = 123,
    NumLock = 144,
    ScrollLock = 145,
    Semicolon = 186,
    EqualSign = 187,
    Comma = 188,
    Dash = 189,
    Period = 190,
    ForwardSlash = 191,
    GraveAccent = 192,
    OpenBracket = 219,
    BackSlash = 220,
    CloseBraket = 221,
    SingleQuote = 222,
    Unknown,
}

impl KeyCode {
    pub fn from_raw_code(i: u8) -> Self {
        use KeyCode::*;
        match i {
            8 => Backspace,
            9 => Tab,
            13 => Enter,
            16 => Shift,
            17 => Ctrl,
            18 => Alt,
            19 => Pause,
            20 => CapsLock,
            27 => Escape,
            33 => PageUp,
            34 => PageDown,
            35 => End,
            36 => Home,
            37 => LeftArrow,
            38 => UpArrow,
            39 => RightArrow,
            40 => DownArrow,
            45 => Insert,
            46 => Delete,
            48 => Num0,
            49 => Num1,
            50 => Num2,
            51 => Num3,
            52 => Num4,
            53 => Num5,
            54 => Num6,
            55 => Num7,
            56 => Num8,
            57 => Num9,
            65 => A,
            66 => B,
            67 => C,
            68 => D,
            69 => E,
            70 => F,
            71 => G,
            72 => H,
            73 => I,
            74 => J,
            75 => K,
            76 => L,
            77 => M,
            78 => N,
            79 => O,
            80 => P,
            81 => Q,
            82 => R,
            83 => S,
            84 => T,
            85 => U,
            86 => V,
            87 => W,
            88 => X,
            89 => Y,
            90 => Z,
            91 => LeftWindow,
            92 => RightWindow,
            93 => SelectKey,
            96 => Numpad0,
            97 => Numpad1,
            98 => Numpad2,
            99 => Numpad3,
            100 => Numpad4,
            101 => Numpad5,
            102 => Numpad6,
            103 => Numpad7,
            104 => Numpad8,
            105 => Numpad9,
            106 => Multiply,
            107 => Add,
            109 => Subtract,
            110 => DecimalPoint,
            111 => Divide,
            112 => F1,
            113 => F2,
            114 => F3,
            115 => F4,
            116 => F5,
            117 => F6,
            118 => F7,
            119 => F8,
            120 => F9,
            121 => F10,
            122 => F11,
            123 => F12,
            144 => NumLock,
            145 => ScrollLock,
            186 => Semicolon,
            187 => EqualSign,
            188 => Comma,
            189 => Dash,
            190 => Period,
            191 => ForwardSlash,
            192 => GraveAccent,
            219 => OpenBracket,
            220 => BackSlash,
            221 => CloseBraket,
            222 => SingleQuote,
            _ => Unknown,
        }
    }

    // get the raw code
    pub fn raw_code(&self) -> u32 {
        *self as u32
    }
}

pub(crate) fn event_meta(event: &UserEvent) -> (bool, EventPriority) {
    use EventPriority::*;

    match event.name {
        // clipboard
        "copy" | "cut" | "paste" => (true, Medium),

        // Composition
        "compositionend" | "compositionstart" | "compositionupdate" => (true, Low),

        // Keyboard
        "keydown" | "keypress" | "keyup" => (true, High),

        // Focus
        "focus" | "blur" => (true, Low),

        // Form
        "change" | "input" | "invalid" | "reset" | "submit" => (true, Medium),

        // Mouse
        "click" | "contextmenu" | "doubleclick" | "drag" | "dragend" | "dragenter" | "dragexit"
        | "dragleave" | "dragover" | "dragstart" | "drop" | "mousedown" | "mouseenter"
        | "mouseleave" | "mouseout" | "mouseover" | "mouseup" => (true, High),

        "mousemove" => (false, Medium),

        // Pointer
        "pointerdown" | "pointermove" | "pointerup" | "pointercancel" | "gotpointercapture"
        | "lostpointercapture" | "pointerenter" | "pointerleave" | "pointerover" | "pointerout" => {
            (true, Medium)
        }

        // Selection
        "select" | "touchcancel" | "touchend" => (true, Medium),

        // Touch
        "touchmove" | "touchstart" => (true, Medium),

        // Wheel
        "scroll" | "wheel" => (false, Medium),

        // Media
        "abort" | "canplay" | "canplaythrough" | "durationchange" | "emptied" | "encrypted"
        | "ended" | "error" | "loadeddata" | "loadedmetadata" | "loadstart" | "pause" | "play"
        | "playing" | "progress" | "ratechange" | "seeked" | "seeking" | "stalled" | "suspend"
        | "timeupdate" | "volumechange" | "waiting" => (true, Medium),

        // Animation
        "animationstart" | "animationend" | "animationiteration" => (true, Medium),

        // Transition
        "transitionend" => (true, Medium),

        // Toggle
        "toggle" => (true, Medium),

        _ => (true, Low),
    }
}
