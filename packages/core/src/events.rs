//! This module provides a set of common events for all Dioxus apps to target, regardless of host platform.
//! -------------------------------------------------------------------------------------------------------
//!
//! 3rd party renderers are responsible for converting their native events into these virtual event types. Events might
//! be heavy or need to interact through FFI, so the events themselves are designed to be lazy.
use crate::innerlude::{ElementId, ScopeId};

#[derive(Debug)]
pub struct UiEvent {
    /// The originator of the event trigger
    pub scope: ScopeId,

    /// The optional real node associated with the trigger
    pub mounted_dom_id: Option<ElementId>,

    pub listener_type: &'static str,

    /// The type of event
    pub event: SyntheticEvent,
}

pub enum SyntheticEvent {
    KeyboardEvent(on::KeyboardEvent),
    TouchEvent(on::TouchEvent),
    MouseEvent(on::MouseEvent),
    ClipboardEvent(on::ClipboardEvent),
    CompositionEvent(on::CompositionEvent),
    FocusEvent(on::FocusEvent),
    FormEvent(on::FormEvent),
    SelectionEvent(on::SelectionEvent),
    WheelEvent(on::WheelEvent),
    MediaEvent(on::MediaEvent),
    AnimationEvent(on::AnimationEvent),
    TransitionEvent(on::TransitionEvent),
    ToggleEvent(on::ToggleEvent),
    PointerEvent(on::PointerEvent),
    // ImageEvent(event_data::ImageEvent),
}

impl std::fmt::Debug for SyntheticEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            SyntheticEvent::ClipboardEvent(_) => "ClipboardEvent",
            SyntheticEvent::CompositionEvent(_) => "CompositionEvent",
            SyntheticEvent::KeyboardEvent(_) => "KeyboardEvent",
            SyntheticEvent::FocusEvent(_) => "FocusEvent",
            SyntheticEvent::FormEvent(_) => "FormEvent",
            SyntheticEvent::SelectionEvent(_) => "SelectionEvent",
            SyntheticEvent::TouchEvent(_) => "TouchEvent",
            SyntheticEvent::WheelEvent(_) => "WheelEvent",
            SyntheticEvent::MediaEvent(_) => "MediaEvent",
            SyntheticEvent::AnimationEvent(_) => "AnimationEvent",
            SyntheticEvent::TransitionEvent(_) => "TransitionEvent",
            SyntheticEvent::ToggleEvent(_) => "ToggleEvent",
            SyntheticEvent::MouseEvent(_) => "MouseEvent",
            SyntheticEvent::PointerEvent(_) => "PointerEvent",
        };

        f.debug_struct("VirtualEvent").field("type", &name).finish()
    }
}

pub mod on {
    //! This module defines the synthetic events that all Dioxus apps enable. No matter the platform, every dioxus renderer
    //! will implement the same events and same behavior (bubbling, cancelation, etc).
    //!
    //! Synthetic events are immutable and wrapped in Arc. It is the intention for Dioxus renderers to re-use the underyling
    //! Arc allocation through "get_mut"
    //!
    //! React recently dropped support for re-using event allocation and just passes the real event along.

    #![allow(unused)]
    use bumpalo::boxed::Box as BumpBox;
    use std::{any::Any, cell::RefCell, fmt::Debug, ops::Deref, rc::Rc};

    use crate::{
        innerlude::NodeFactory,
        innerlude::{Attribute, ElementId, Listener, VNode},
    };
    use std::cell::Cell;

    use super::SyntheticEvent;

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
                pub struct $wrapper(pub Rc<dyn $eventdata>);

                // todo: derefing to the event is fine (and easy) but breaks some IDE stuff like (go to source)
                // going to source in fact takes you to the source of Rc which is... less than useful
                // Either we ask for this to be changed in Rust-analyzer or manually impkement the trait
                impl Deref for $wrapper {
                    type Target = Rc<dyn $eventdata>;
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

                        let event_name = stringify!($name);
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
            /// oninput
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
            ///
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

    pub trait GenericEventInner {
        /// Return a reference to the raw event. User will need to downcast the event to the right platform-specific type.
        fn raw_event(&self) -> &dyn Any;
        /// Returns whether or not a specific event is a bubbling event
        fn bubbles(&self) -> bool;
        /// Sets or returns whether the event should propagate up the hierarchy or not
        fn cancel_bubble(&self);
        /// Returns whether or not an event can have its default action prevented
        fn cancelable(&self) -> bool;
        /// Returns whether the event is composed or not
        fn composed(&self) -> bool;

        // Currently not supported because those no way we could possibly support it
        // just cast the event to the right platform-specific type and return it
        // /// Returns the event's path
        // fn composed_path(&self) -> String;

        /// Returns the element whose event listeners triggered the event
        fn current_target(&self);
        /// Returns whether or not the preventDefault method was called for the event
        fn default_prevented(&self) -> bool;
        /// Returns which phase of the event flow is currently being evaluated
        fn event_phase(&self) -> u16;
        /// Returns whether or not an event is trusted
        fn is_trusted(&self) -> bool;
        /// Cancels the event if it is cancelable, meaning that the default action that belongs to the event will
        fn prevent_default(&self);
        /// Prevents other listeners of the same event from being called
        fn stop_immediate_propagation(&self);
        /// Prevents further propagation of an event during event flow
        fn stop_propagation(&self);
        /// Returns the element that triggered the event
        fn target(&self);
        /// Returns the time (in milliseconds relative to the epoch) at which the event was created
        fn time_stamp(&self) -> f64;
    }

    pub trait ClipboardEventInner {
        // DOMDataTransfer clipboardData
    }

    pub trait CompositionEventInner {
        fn data(&self) -> String;
    }

    pub trait KeyboardEventInner {
        fn alt_key(&self) -> bool;

        fn char_code(&self) -> u32;

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
        fn key(&self) -> String;

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
        fn key_code(&self) -> KeyCode;

        /// Check if the ctrl key was pressed down
        fn ctrl_key(&self) -> bool;

        fn get_modifier_state(&self, key_code: &str) -> bool;

        fn locale(&self) -> String;
        fn location(&self) -> usize;
        fn meta_key(&self) -> bool;
        fn repeat(&self) -> bool;
        fn shift_key(&self) -> bool;
        fn which(&self) -> usize;
    }

    pub trait FocusEventInner {
        /* DOMEventInnerTarget relatedTarget */
    }

    pub trait FormEventInner {
        fn value(&self) -> String;
    }

    pub trait MouseEventInner {
        fn alt_key(&self) -> bool;
        fn button(&self) -> i16;
        fn buttons(&self) -> u16;
        /// Get the X coordinate of the mouse relative to the window
        fn client_x(&self) -> i32;
        fn client_y(&self) -> i32;
        fn ctrl_key(&self) -> bool;
        fn meta_key(&self) -> bool;
        fn page_x(&self) -> i32;
        fn page_y(&self) -> i32;
        fn screen_x(&self) -> i32;
        fn screen_y(&self) -> i32;
        fn shift_key(&self) -> bool;
        fn get_modifier_state(&self, key_code: &str) -> bool;
    }

    pub trait PointerEventInner {
        // Mouse only
        fn alt_key(&self) -> bool;
        fn button(&self) -> i16;
        fn buttons(&self) -> u16;
        fn client_x(&self) -> i32;
        fn client_y(&self) -> i32;
        fn ctrl_key(&self) -> bool;
        fn meta_key(&self) -> bool;
        fn page_x(&self) -> i32;
        fn page_y(&self) -> i32;
        fn screen_x(&self) -> i32;
        fn screen_y(&self) -> i32;
        fn shift_key(&self) -> bool;
        fn get_modifier_state(&self, key_code: &str) -> bool;
        fn pointer_id(&self) -> i32;
        fn width(&self) -> i32;
        fn height(&self) -> i32;
        fn pressure(&self) -> f32;
        fn tangential_pressure(&self) -> f32;
        fn tilt_x(&self) -> i32;
        fn tilt_y(&self) -> i32;
        fn twist(&self) -> i32;
        fn pointer_type(&self) -> String;
        fn is_primary(&self) -> bool;
    }

    pub trait SelectionEventInner {}

    pub trait TouchEventInner {
        fn alt_key(&self) -> bool;
        fn ctrl_key(&self) -> bool;
        fn meta_key(&self) -> bool;
        fn shift_key(&self) -> bool;
        fn get_modifier_state(&self, key_code: &str) -> bool;
        // changedTouches: DOMTouchList,
        // targetTouches: DOMTouchList,
        // touches: DOMTouchList,
    }

    pub trait UIEventInner {
        // DOMAbstractView view
        fn detail(&self) -> i32;
    }

    pub trait WheelEventInner {
        fn delta_mode(&self) -> u32;
        fn delta_x(&self) -> f64;
        fn delta_y(&self) -> f64;
        fn delta_z(&self) -> f64;
    }

    pub trait MediaEventInner {}

    pub trait ImageEventInner {
        //     load error
    }

    pub trait AnimationEventInner {
        fn animation_name(&self) -> String;
        fn pseudo_element(&self) -> String;
        fn elapsed_time(&self) -> f32;
    }

    pub trait TransitionEventInner {
        fn property_name(&self) -> String;
        fn pseudo_element(&self) -> String;
        fn elapsed_time(&self) -> f32;
    }

    pub trait ToggleEventInner {}

    pub use util::KeyCode;
    mod util {

        #[derive(Clone, Copy)]
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
            fn raw_code(&self) -> u32 {
                *self as u32
            }
        }
    }
}
