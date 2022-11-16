#[cfg_attr(
    feature = "serialize",
    derive(serde_repr::Serialize_repr, serde_repr::Deserialize_repr)
)]
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
    #[cfg_attr(feature = "serialize", serde(other))]
    Unknown,
}
