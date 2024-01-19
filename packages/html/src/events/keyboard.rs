use dioxus_core::Event;
use keyboard_types::{Code, Key, Location, Modifiers};
use std::fmt::Debug;

use crate::prelude::ModifiersInteraction;

#[cfg(feature = "serialize")]
fn resilient_deserialize_code<'de, D>(deserializer: D) -> Result<Code, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    // If we fail to deserialize the code for any reason, just return Unidentified instead of failing.
    Ok(Code::deserialize(deserializer).unwrap_or(Code::Unidentified))
}

pub type KeyboardEvent = Event<KeyboardData>;
pub struct KeyboardData {
    inner: Box<dyn HasKeyboardData>,
}

impl<E: HasKeyboardData> From<E> for KeyboardData {
    fn from(e: E) -> Self {
        Self { inner: Box::new(e) }
    }
}

impl std::fmt::Debug for KeyboardData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyboardData")
            .field("key", &self.key())
            .field("code", &self.code())
            .field("modifiers", &self.modifiers())
            .field("location", &self.location())
            .field("is_auto_repeating", &self.is_auto_repeating())
            .finish()
    }
}

impl PartialEq for KeyboardData {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
            && self.code() == other.code()
            && self.modifiers() == other.modifiers()
            && self.location() == other.location()
            && self.is_auto_repeating() == other.is_auto_repeating()
    }
}

impl KeyboardData {
    /// Create a new KeyboardData
    pub fn new(inner: impl HasKeyboardData + 'static) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }

    /// The value of the key pressed by the user, taking into consideration the state of modifier keys such as Shift as well as the keyboard locale and layout.
    pub fn key(&self) -> Key {
        self.inner.key()
    }

    /// A physical key on the keyboard (as opposed to the character generated by pressing the key). In other words, this property returns a value that isn't altered by keyboard layout or the state of the modifier keys.
    pub fn code(&self) -> Code {
        self.inner.code()
    }

    /// The location of the key on the keyboard or other input device.
    pub fn location(&self) -> Location {
        self.inner.location()
    }

    /// `true` iff the key is being held down such that it is automatically repeating.
    pub fn is_auto_repeating(&self) -> bool {
        self.inner.is_auto_repeating()
    }

    /// Downcast this KeyboardData to a concrete type.
    pub fn downcast<T: 'static>(&self) -> Option<&T> {
        self.inner.as_any().downcast_ref::<T>()
    }
}

impl ModifiersInteraction for KeyboardData {
    fn modifiers(&self) -> Modifiers {
        self.inner.modifiers()
    }
}

#[cfg(feature = "serialize")]
/// A serialized version of KeyboardData
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Clone)]
pub struct SerializedKeyboardData {
    char_code: u32,
    key: String,
    key_code: KeyCode,
    #[serde(deserialize_with = "resilient_deserialize_code")]
    code: Code,
    alt_key: bool,
    ctrl_key: bool,
    meta_key: bool,
    shift_key: bool,
    location: usize,
    repeat: bool,
    which: usize,
}

#[cfg(feature = "serialize")]
impl SerializedKeyboardData {
    /// Create a new SerializedKeyboardData
    pub fn new(
        key: Key,
        code: Code,
        location: Location,
        is_auto_repeating: bool,
        modifiers: Modifiers,
    ) -> Self {
        Self {
            char_code: key.legacy_charcode(),
            key: key.to_string(),
            key_code: KeyCode::from_raw_code(
                std::convert::TryInto::try_into(key.legacy_keycode())
                    .expect("could not convert keycode to u8"),
            ),
            code,
            alt_key: modifiers.contains(Modifiers::ALT),
            ctrl_key: modifiers.contains(Modifiers::CONTROL),
            meta_key: modifiers.contains(Modifiers::META),
            shift_key: modifiers.contains(Modifiers::SHIFT),
            location: crate::input_data::encode_key_location(location),
            repeat: is_auto_repeating,
            which: std::convert::TryInto::try_into(key.legacy_charcode())
                .expect("could not convert charcode to usize"),
        }
    }
}

#[cfg(feature = "serialize")]
impl From<&KeyboardData> for SerializedKeyboardData {
    fn from(data: &KeyboardData) -> Self {
        Self::new(
            data.key(),
            data.code(),
            data.location(),
            data.is_auto_repeating(),
            data.modifiers(),
        )
    }
}

#[cfg(feature = "serialize")]
impl HasKeyboardData for SerializedKeyboardData {
    fn key(&self) -> Key {
        std::str::FromStr::from_str(&self.key).unwrap_or(Key::Unidentified)
    }

    fn code(&self) -> Code {
        self.code
    }

    fn location(&self) -> Location {
        crate::input_data::decode_key_location(self.location)
    }

    fn is_auto_repeating(&self) -> bool {
        self.repeat
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(feature = "serialize")]
impl ModifiersInteraction for SerializedKeyboardData {
    fn modifiers(&self) -> Modifiers {
        let mut modifiers = Modifiers::empty();

        if self.alt_key {
            modifiers.insert(Modifiers::ALT);
        }
        if self.ctrl_key {
            modifiers.insert(Modifiers::CONTROL);
        }
        if self.meta_key {
            modifiers.insert(Modifiers::META);
        }
        if self.shift_key {
            modifiers.insert(Modifiers::SHIFT);
        }

        modifiers
    }
}

#[cfg(feature = "serialize")]
impl serde::Serialize for KeyboardData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SerializedKeyboardData::from(self).serialize(serializer)
    }
}

#[cfg(feature = "serialize")]
impl<'de> serde::Deserialize<'de> for KeyboardData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let data = SerializedKeyboardData::deserialize(deserializer)?;
        Ok(Self {
            inner: Box::new(data),
        })
    }
}

impl_event! {
    KeyboardData;

    /// onkeydown
    onkeydown

    /// onkeypress
    onkeypress

    /// onkeyup
    onkeyup
}

pub trait HasKeyboardData: ModifiersInteraction + std::any::Any {
    /// The value of the key pressed by the user, taking into consideration the state of modifier keys such as Shift as well as the keyboard locale and layout.
    fn key(&self) -> Key;

    /// A physical key on the keyboard (as opposed to the character generated by pressing the key). In other words, this property returns a value that isn't altered by keyboard layout or the state of the modifier keys.
    fn code(&self) -> Code;

    /// The location of the key on the keyboard or other input device.
    fn location(&self) -> Location;

    /// `true` iff the key is being held down such that it is automatically repeating.
    fn is_auto_repeating(&self) -> bool;

    /// return self as Any
    fn as_any(&self) -> &dyn std::any::Any;
}

#[cfg(feature = "serialize")]
impl<'de> serde::Deserialize<'de> for KeyCode {
    fn deserialize<D>(deserializer: D) -> Result<KeyCode, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // We could be deserializing a unicode character, so we need to use u64 even if the output only takes u8
        let value = u64::deserialize(deserializer)?;

        if let Ok(smaller_uint) = value.try_into() {
            Ok(KeyCode::from_raw_code(smaller_uint))
        } else {
            Ok(KeyCode::Unknown)
        }
    }
}

#[cfg_attr(feature = "serialize", derive(serde_repr::Serialize_repr))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum KeyCode {
    // That key has no keycode, = 0
    // break, = 3
    // backspace / delete, = 8
    // tab, = 9
    // clear, = 12
    // enter, = 13
    // shift, = 16
    // ctrl, = 17
    // alt, = 18
    // pause/break, = 19
    // caps lock, = 20
    // hangul, = 21
    // hanja, = 25
    // escape, = 27
    // conversion, = 28
    // non-conversion, = 29
    // spacebar, = 32
    // page up, = 33
    // page down, = 34
    // end, = 35
    // home, = 36
    // left arrow, = 37
    // up arrow, = 38
    // right arrow, = 39
    // down arrow, = 40
    // select, = 41
    // print, = 42
    // execute, = 43
    // Print Screen, = 44
    // insert, = 45
    // delete, = 46
    // help, = 47
    // 0, = 48
    // 1, = 49
    // 2, = 50
    // 3, = 51
    // 4, = 52
    // 5, = 53
    // 6, = 54
    // 7, = 55
    // 8, = 56
    // 9, = 57
    // :, = 58
    // semicolon (firefox), equals, = 59
    // <, = 60
    // equals (firefox), = 61
    // ß, = 63
    // @ (firefox), = 64
    // a, = 65
    // b, = 66
    // c, = 67
    // d, = 68
    // e, = 69
    // f, = 70
    // g, = 71
    // h, = 72
    // i, = 73
    // j, = 74
    // k, = 75
    // l, = 76
    // m, = 77
    // n, = 78
    // o, = 79
    // p, = 80
    // q, = 81
    // r, = 82
    // s, = 83
    // t, = 84
    // u, = 85
    // v, = 86
    // w, = 87
    // x, = 88
    // y, = 89
    // z, = 90
    // Windows Key / Left ⌘ / Chromebook Search key, = 91
    // right window key, = 92
    // Windows Menu / Right ⌘, = 93
    // sleep, = 95
    // numpad 0, = 96
    // numpad 1, = 97
    // numpad 2, = 98
    // numpad 3, = 99
    // numpad 4, = 100
    // numpad 5, = 101
    // numpad 6, = 102
    // numpad 7, = 103
    // numpad 8, = 104
    // numpad 9, = 105
    // multiply, = 106
    // add, = 107
    // numpad period (firefox), = 108
    // subtract, = 109
    // decimal point, = 110
    // divide, = 111
    // f1, = 112
    // f2, = 113
    // f3, = 114
    // f4, = 115
    // f5, = 116
    // f6, = 117
    // f7, = 118
    // f8, = 119
    // f9, = 120
    // f10, = 121
    // f11, = 122
    // f12, = 123
    // f13, = 124
    // f14, = 125
    // f15, = 126
    // f16, = 127
    // f17, = 128
    // f18, = 129
    // f19, = 130
    // f20, = 131
    // f21, = 132
    // f22, = 133
    // f23, = 134
    // f24, = 135
    // f25, = 136
    // f26, = 137
    // f27, = 138
    // f28, = 139
    // f29, = 140
    // f30, = 141
    // f31, = 142
    // f32, = 143
    // num lock, = 144
    // scroll lock, = 145
    // airplane mode, = 151
    // ^, = 160
    // !, = 161
    // ؛ (arabic semicolon), = 162
    // #, = 163
    // $, = 164
    // ù, = 165
    // page backward, = 166
    // page forward, = 167
    // refresh, = 168
    // closing paren (AZERTY), = 169
    // *, = 170
    // ~ + * key, = 171
    // home key, = 172
    // minus (firefox), mute/unmute, = 173
    // decrease volume level, = 174
    // increase volume level, = 175
    // next, = 176
    // previous, = 177
    // stop, = 178
    // play/pause, = 179
    // e-mail, = 180
    // mute/unmute (firefox), = 181
    // decrease volume level (firefox), = 182
    // increase volume level (firefox), = 183
    // semi-colon / ñ, = 186
    // equal sign, = 187
    // comma, = 188
    // dash, = 189
    // period, = 190
    // forward slash / ç, = 191
    // grave accent / ñ / æ / ö, = 192
    // ?, / or °, = 193
    // numpad period (chrome), = 194
    // open bracket, = 219
    // back slash, = 220
    // close bracket / å, = 221
    // single quote / ø / ä, = 222
    // `, = 223
    // left or right ⌘ key (firefox), = 224
    // altgr, = 225
    // < /git >, left back slash, = 226
    // GNOME Compose Key, = 230
    // ç, = 231
    // XF86Forward, = 233
    // XF86Back, = 234
    // non-conversion, = 235
    // alphanumeric, = 240
    // hiragana/katakana, = 242
    // half-width/full-width, = 243
    // kanji, = 244
    // unlock trackpad (Chrome/Edge), = 251
    // toggle touchpad, = 255
    NA = 0,
    Break = 3,
    Backspace = 8,
    Tab = 9,
    Clear = 12,
    Enter = 13,
    Shift = 16,
    Ctrl = 17,
    Alt = 18,
    Pause = 19,
    CapsLock = 20,
    // hangul, = 21
    // hanja, = 25
    Escape = 27,
    // conversion, = 28
    // non-conversion, = 29
    Space = 32,
    PageUp = 33,
    PageDown = 34,
    End = 35,
    Home = 36,
    LeftArrow = 37,
    UpArrow = 38,
    RightArrow = 39,
    DownArrow = 40,
    // select, = 41
    // print, = 42
    // execute, = 43
    // Print Screen, = 44
    Insert = 45,
    Delete = 46,
    // help, = 47
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
    // :, = 58
    // semicolon (firefox), equals, = 59
    // <, = 60
    // equals (firefox), = 61
    // ß, = 63
    // @ (firefox), = 64
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
    // f13, = 124
    // f14, = 125
    // f15, = 126
    // f16, = 127
    // f17, = 128
    // f18, = 129
    // f19, = 130
    // f20, = 131
    // f21, = 132
    // f22, = 133
    // f23, = 134
    // f24, = 135
    // f25, = 136
    // f26, = 137
    // f27, = 138
    // f28, = 139
    // f29, = 140
    // f30, = 141
    // f31, = 142
    // f32, = 143
    NumLock = 144,
    ScrollLock = 145,
    // airplane mode, = 151
    // ^, = 160
    // !, = 161
    // ؛ (arabic semicolon), = 162
    // #, = 163
    // $, = 164
    // ù, = 165
    // page backward, = 166
    // page forward, = 167
    // refresh, = 168
    // closing paren (AZERTY), = 169
    // *, = 170
    // ~ + * key, = 171
    // home key, = 172
    // minus (firefox), mute/unmute, = 173
    // decrease volume level, = 174
    // increase volume level, = 175
    // next, = 176
    // previous, = 177
    // stop, = 178
    // play/pause, = 179
    // e-mail, = 180
    // mute/unmute (firefox), = 181
    // decrease volume level (firefox), = 182
    // increase volume level (firefox), = 183
    Semicolon = 186,
    EqualSign = 187,
    Comma = 188,
    Dash = 189,
    Period = 190,
    ForwardSlash = 191,
    GraveAccent = 192,
    // ?, / or °, = 193
    // numpad period (chrome), = 194
    OpenBracket = 219,
    BackSlash = 220,
    CloseBraket = 221,
    SingleQuote = 222,
    // `, = 223
    // left or right ⌘ key (firefox), = 224
    // altgr, = 225
    // < /git >, left back slash, = 226
    // GNOME Compose Key, = 230
    // ç, = 231
    // XF86Forward, = 233
    // XF86Back, = 234
    // non-conversion, = 235
    // alphanumeric, = 240
    // hiragana/katakana, = 242
    // half-width/full-width, = 243
    // kanji, = 244
    // unlock trackpad (Chrome/Edge), = 251
    // toggle touchpad, = 255
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
